# AIL Architecture

> **ail** — Alexander's Impressive Loops  
> *The control plane for how agents behave after the human stops typing.*

---

> ⚠️ **This document is a work in progress.**
>
> The rationale and high-level architecture described here reflect decisions made before significant code has been written. Implementation details, module boundaries, and crate structure will be added incrementally as the v0.0.1 spike produces empirical findings. See `SPIKE.md` (forthcoming).

---

## Table of Contents

1. [Why Rust](#1-why-rust)
2. [The Control Plane / Agent Boundary](#2-the-control-plane--agent-boundary)
3. [High-Level System Model](#3-high-level-system-model)
4. [Runner Adapter Architecture](#4-runner-adapter-architecture)
5. [Stream Parsing Isolation](#5-stream-parsing-isolation)
6. [Spec Coverage Testing](#6-spec-coverage-testing)
7. [Known Alternatives Considered](#7-known-alternatives-considered)

---

## 1. Why Rust

The implementation language of `ail` is a strategic decision, not a convenience one. This section records the reasoning so it is available to contributors who ask, and to future maintainers who might reconsider it.

### The Core Claim

`ail` is a control plane, not a tool. The distinction matters:

- **A tool** is something a developer runs locally to get work done. Memory footprint, startup time, and resource efficiency are nice-to-haves.
- **A control plane** is something an enterprise deploys at scale — one instance per active agent session, potentially thousands of concurrent instances, running continuously. Memory footprint, startup time, and resource efficiency are line items in a procurement conversation.

Rust is the only mainstream systems language that makes `ail` credibly positionable as infrastructure. This is the primary rationale.

### The Memory Argument

A Node.js process running the Claude Code SDK has a baseline resident set size (RSS) of approximately 80–120MB before performing any useful work. This reflects the V8 heap, the event loop, JIT compilation warm-up, and the TypeScript runtime overhead. Claude Code itself is an 11MB compiled TypeScript file — the orchestration layer adds significantly more on top.

A Rust binary performing equivalent work — spawning a subprocess, reading NDJSON from stdout, evaluating conditional logic, dispatching follow-up prompts — has a steady-state RSS of approximately 2–5MB. This is not an optimisation. It is the language's default behaviour.

**At scale, the delta is not marginal:**

| Concurrent sessions | Node.js orchestration RAM | Rust orchestration RAM | Savings |
|---|---|---|---|
| 100 | ~10 GB | ~250 MB | ~97% |
| 1,000 | ~100 GB | ~2.5 GB | ~97% |
| 10,000 | ~1 TB | ~25 GB | ~97% |

At current cloud infrastructure memory pricing, the difference between the Node and Rust columns at 10,000 concurrent sessions represents a cost delta in the hundreds of thousands of dollars annually. For an enterprise running a million-dollar annual Claude compute budget, the orchestration layer's memory profile will appear in cost reports. An 80%+ reduction in orchestration overhead is a genuine procurement argument.

### Beyond Memory

Memory is the most legible argument, but not the only one.

**CPU predictability.** Rust has no garbage collector. There are no GC pause events introducing latency jitter into what should be deterministic orchestration. When `ail` fires a pipeline step, it fires it. There is no "GC decided now was a good time" risk.

**Cold-start time.** Containerised deployments that need to spin up quickly under burst load benefit from Rust's near-instant binary startup. No JIT warm-up, no module resolution, no runtime initialisation overhead.

**Static binary distribution.** A Rust release build is a single static binary with no runtime dependencies. It can be shipped as a 5–10MB download, dropped into a container with no additional layer, or distributed via package managers without requiring Node, Python, or any other runtime to be pre-installed. This matters significantly for enterprise deployment where runtime dependency management is a friction point.

**Embeddability.** If `ail` eventually needs to run embedded — as a sidecar process, as a library, or as a component in a larger orchestration system — Rust's FFI capabilities and its lack of a managed runtime make this tractable. Node does not embed cleanly.

**Supply chain and resource constraints.** Global RAM and storage constraints are not hypothetical. As hardware supply chains remain under pressure and compute costs remain elevated, the trend across enterprise infrastructure is toward smaller, more efficient workloads. Writing for eventual resource efficiency now costs little. Rewriting later, after adoption, costs enormously.

### The Risk Acknowledged

The most credible argument against Rust here is interface stability risk. `ail` depends on the Claude CLI's `--output-format stream-json` interface, which is not a formally versioned public API. The official Claude Code SDK (Python and TypeScript) would absorb interface changes transparently; Rust code that hand-rolls NDJSON parsing would break.

This risk is real and is mitigated by two design decisions:

1. Stream parsing is isolated behind a trait boundary (see §5). Interface changes affect one module.
2. An integration test suite that runs against the live CLI is maintained and monitored. Breaking changes surface immediately rather than in production.

The risk does not change the language decision. It changes the architecture within that decision.

---

## 2. The Control Plane / Agent Boundary

`ail` operates at the boundary between a human and an underlying agent. It is important to be precise about where `ail` lives and where the agent lives, because they must remain separable.

```
┌─────────────────────────────────────────────────────┐
│                   Human                             │
└────────────────────────┬────────────────────────────┘
                         │ prompt
                         ▼
┌─────────────────────────────────────────────────────┐
│                   ail (control plane)               │
│                                                     │
│  ┌─────────────┐    ┌──────────────────────────┐   │
│  │ YAML parser │    │ Pipeline executor        │   │
│  │ .ail.yaml   │───▶│ - step sequencing        │   │
│  └─────────────┘    │ - condition evaluation   │   │
│                     │ - on_result branching    │   │
│  ┌─────────────┐    │ - HITL gate management   │   │
│  │ TUI         │◀───│ - template resolution    │   │
│  │ (ratatui)   │    └──────────┬───────────────┘   │
│  └─────────────┘               │                   │
└───────────────────────────────┬┼───────────────────┘
                                ││ stdin/stdout (NDJSON)
                                ▼▼
┌─────────────────────────────────────────────────────┐
│             Underlying Agent (runner)               │
│          e.g. Claude CLI, Aider, OpenCode           │
└─────────────────────────────────────────────────────┘
```

The agent is always a separate process. `ail` communicates with it over stdin/stdout using the runner's structured output interface. This boundary is not an implementation detail — it is a design principle.

**Why the hard boundary matters:**

- The agent can be upgraded, swapped, or replaced without touching `ail`'s pipeline logic.
- `ail` does not need to understand anything about the agent's internals — only its input/output protocol.
- The agent's memory footprint is the agent's business. `ail`'s footprint is `ail`'s business. They do not compound.
- In a multi-agent deployment, multiple agents can share a single `ail` control plane, or each can have a dedicated instance — the architecture supports both.

---

## 3. High-Level System Model

At v0.0.1, the system is straightforward. Complexity is additive, not foundational.

### Startup

```
1. Resolve pipeline file (discovery order per SPEC §3.1)
2. Parse and validate .ail.yaml
3. Resolve FROM chain if present (v0.1+)
4. Initialise runner adapter for configured runner
5. Start TUI
6. Enter main loop
```

### Main Loop

```
1. Pass human prompt to runner
2. Stream runner output to TUI (passthrough)
3. Receive completion signal (result event / exit code 0)
4. For each step in pipeline:
   a. Evaluate condition — skip if false
   b. Resolve template variables
   c. Invoke runner with step prompt
   d. Stream step output to TUI
   e. Evaluate on_result — branch if match
   f. Execute then: chain if present (private, no hooks)
5. Return control to human
```

### Module Sketch (v0.0.1)

```
src/
  main.rs          — Entry point. CLI argument parsing (clap). Wires modules together.
  config.rs        — .ail.yaml parsing (serde_yaml). Validation. Discovery logic.
  runner.rs        — Runner trait definition. Claude CLI adapter implementation.
  pipeline.rs      — Step execution loop. Condition evaluation. on_result branching.
                     Template variable resolution.
  tui.rs           — Terminal output (ratatui or simpler for v0.0.1).
  session.rs       — Session state. In-memory tool allowlist. run_id generation.
  error.rs         — Error types. on_error dispatch.
```

This is intentionally flat. Premature module decomposition before the spike validates the core loop is waste. The module boundaries above are informed guesses that should be revisited after v0.0.1 is running.

---

## 4. Runner Adapter Architecture

The `Runner` trait is the seam between `ail`'s pipeline executor and the underlying agent. Everything the pipeline executor needs from a runner is expressed through this trait. Nothing else leaks through.

### The Trait (Sketch)

```rust
/// A runner is a subprocess-based agent that ail can orchestrate.
/// Each runner implementation knows how to invoke the underlying CLI tool,
/// stream its output, signal completion, and handle tool permissions.
pub trait Runner: Send + Sync {
    /// Invoke the runner with a prompt, streaming output via callback.
    /// Returns when the runner signals completion.
    fn invoke(
        &self,
        prompt: &str,
        config: &StepConfig,
        on_event: impl Fn(RunnerEvent) + Send,
    ) -> Result<RunnerOutput, RunnerError>;

    /// Name of this runner, for logging and TUI display.
    fn name(&self) -> &str;

    /// Capabilities this runner supports.
    fn capabilities(&self) -> RunnerCapabilities;
}

pub struct RunnerCapabilities {
    pub structured_output: bool,   // --output-format stream-json
    pub bidirectional_stdin: bool, // --input-format stream-json
    pub tool_permissions: bool,    // --permission-prompt-tool stdio
    pub session_continuity: bool,  // session_id resumption
}

pub enum RunnerEvent {
    TextDelta(String),             // streaming text fragment
    ToolUse(ToolUseEvent),         // tool call intercepted
    PermissionRequest(PermissionEvent), // HITL permission gate
    Complete(RunnerOutput),        // final result
    Error(RunnerError),
}

pub struct RunnerOutput {
    pub response: String,          // final text response
    pub cost_usd: Option<f64>,     // total_cost_usd if available
    pub session_id: Option<String>,
}
```

### Three Tiers in Code

**Tier 1 — First-class (built-in adapters):**
Implementations of `Runner` that ship with `ail`. `ClaudeCliRunner` is the first. Each knows the full flag set and event format for its target CLI tool. Maintained by the core team, tested against every release.

**Tier 2 — Minimum compliance:**
A `MinimalCliRunner` that implements `Runner` using only exit code 0 as the completion signal and stdout capture as the response. Any CLI tool that accepts a prompt via a flag and exits cleanly works with this adapter. Capability flags all false.

**Tier 3 — Dynamic adapters:**
Third-party adapters implemented as Rust `cdylib` dynamic libraries, loaded at runtime via `libloading`. They implement the `Runner` trait and expose a known entry point symbol. This is a planned feature — the trait boundary above is designed with it in mind, but the loading mechanism is not yet implemented.

---

## 5. Stream Parsing Isolation

The Claude CLI's `--output-format stream-json` interface is the most fragile dependency in the system. It is not a formally versioned public API. Anthropic can change event shapes, add fields, rename subtypes, or alter the protocol without a breaking-change notice.

The mitigation is **complete isolation**. All NDJSON parsing for the Claude CLI lives in one module (`src/runners/claude/stream.rs` or equivalent). The rest of the codebase — pipeline executor, condition evaluator, TUI, session state — never touches raw JSON. They receive typed `RunnerEvent` values.

```
Raw NDJSON bytes
    ↓
claude/stream.rs   ← THE ONLY MODULE THAT KNOWS ABOUT JSON SHAPES
    ↓
RunnerEvent enum   ← typed, stable, owned by ail
    ↓
pipeline.rs, tui.rs, session.rs, ...
```

When Anthropic changes the stream format, the blast radius is `claude/stream.rs`. The rest of the codebase is unaffected.

This is not an optimisation. It is a maintenance guarantee.

---

## 6. Spec Coverage Testing

### The Problem

A spec without a machine-verifiable completeness signal is a document that drifts. Features get implemented partially, edge cases get skipped, and the only way to know what actually works is to read the code. For a project where the spec *is* the product — where users and contributors rely on it as a contract — this is unacceptable.

### The Solution: `tests/spec_coverage.rs`

`tests/spec_coverage.rs` is a permanent, dedicated integration test file that mirrors the spec section-by-section. It is not a staging area for tests that will later migrate into implementation files. It is a first-class project artifact — as important as the spec itself — and its output is the definitive spec completeness report.

Running `cargo nextest run --test spec_coverage` produces the completeness report. The test output *is* the report. Nothing needs to be generated or maintained separately.

### Naming Convention

Rust test function names must be valid identifiers — they cannot begin with a numeral or contain dots. The section structure is encoded in the module hierarchy instead, which `cargo test` and `cargo-nextest` render as a dotted path in output.

The convention is:

```
tests/spec_coverage.rs

mod spec {
    mod s5_step_specification {       // § 5
        mod s5_5_then_chain {         // § 5.5
            #[test]
            fn short_form_bare_skill() { ... }

            #[test]
            fn full_form_with_config() { ... }

            /// SPEC §5.5 — then: chain is not targetable by run_before/run_after
            #[test]
            #[ignore = "FROM inheritance not yet implemented — SPEC §7"]
            fn not_hookable_by_inheriting_pipeline() { ... }
        }

        mod s5_6_tools {              // § 5.6
            #[test]
            fn allow_list_parsed() { ... }

            #[test]
            fn deny_list_parsed() { ... }

            #[test]
            fn pattern_syntax_passthrough() { ... }

            #[test]
            fn defaults_block_inheritance() { ... }
        }
    }

    mod s13_hitl {                    // § 13
        mod s13_4_permission_flow {   // § 13.4
            #[test]
            fn allow_list_silent() { ... }

            #[test]
            fn deny_list_silent() { ... }

            #[test]
            fn session_allowlist_silent() { ... }

            #[test]
            fn unspecified_tool_triggers_hitl() { ... }
        }
    }
}
```

The output from `cargo nextest run --test spec_coverage` reads as a structured completeness report:

```
test spec::s5_step_specification::s5_5_then_chain::short_form_bare_skill         ... ok
test spec::s5_step_specification::s5_5_then_chain::full_form_with_config          ... ok
test spec::s5_step_specification::s5_5_then_chain::not_hookable_by_inheriting_pipeline ... IGNORED (FROM inheritance not yet implemented — SPEC §7)
test spec::s5_step_specification::s5_6_tools::allow_list_parsed                   ... ok
test spec::s5_step_specification::s5_6_tools::deny_list_parsed                    ... ok
test spec::s5_step_specification::s5_6_tools::pattern_syntax_passthrough          ... ok
test spec::s5_step_specification::s5_6_tools::defaults_block_inheritance          ... ok
test spec::s13_hitl::s13_4_permission_flow::allow_list_silent                     ... ok
test spec::s13_hitl::s13_4_permission_flow::deny_list_silent                      ... ok
test spec::s13_hitl::s13_4_permission_flow::session_allowlist_silent              ... ok
test spec::s13_hitl::s13_4_permission_flow::unspecified_tool_triggers_hitl        ... ok
```

`ok` = implemented and working. `IGNORED` = specced but not yet implemented (the backlog). `FAILED` = broken. The ignored count is the implementation backlog size expressed as a number.

### Why Tests Stay in `spec_coverage.rs` Permanently

It is tempting to treat `spec_coverage.rs` as a staging area — write the test here first, then move it next to the implementation once that's written. This is the wrong model for two reasons.

**The file is a document.** `tests/spec_coverage.rs` is readable as a mirror of the spec. Organised by section number, in spec order, it tells you exactly what is and isn't implemented without running anything. If tests migrate out, this document ceases to exist. You cannot reconstruct the spec completeness picture by reading the file — you can only get it by running a command and mentally reassembling scattered results.

**The tests operate at a different level.** Spec coverage tests are integration tests: they construct a YAML string, run it through the full pipeline, and check observable behaviour against what the spec says. Implementation unit tests in `config.rs` or `pipeline.rs` test internal function correctness — edge cases, error paths, specific parsing behaviour. These are different tests, not the same test in two locations. Both should exist; neither replaces the other.

### What a Spec Coverage Test Looks Like

Each test must exercise the real parser and executor — no mocks. A test that asserts `true` is lying to the reader.

```rust
/// SPEC §5.6 — tools.allow list is parsed and passed to runner as --allowedTools
#[test]
fn s5_6_allow_list_parsed() {
    let yaml = r#"
        version: "0.1"
        pipeline:
          - id: audit
            prompt: "check this"
            tools:
              allow: [Read, Glob]
              deny: [WebFetch]
    "#;
    let config = Config::parse(yaml).expect("valid config should parse");
    let step = &config.pipeline[0];
    assert_eq!(step.tools.allow, vec!["Read", "Glob"]);
    assert_eq!(step.tools.deny, vec!["WebFetch"]);
}
```

The test name maps to a spec section. The doc comment cites the spec section. The assertion checks the behaviour the spec describes. If the spec changes, the test must change to match. If the implementation changes and breaks the spec behaviour, the test fails.

### Planned Features Use `#[ignore]`

Every feature in SPEC.md that is not yet implemented gets a test stub with `#[ignore]` and a reason that cites the spec section:

```rust
/// SPEC §22 — Sealed Steps (Planned Extension)
#[test]
#[ignore = "sealed: not yet implemented — SPEC §22 Planned Extensions"]
fn s22_sealed_step_rejects_run_before_hook() {
    todo!()
}
```

This has two effects. Running `cargo nextest run --test spec_coverage` shows the full backlog as IGNORED tests. Running `cargo nextest run --test spec_coverage --ignored` runs only the unimplemented tests — useful when picking up a new feature to implement.

### The README Status Table

A CI step parses `cargo nextest` output and regenerates the README status table on every commit. Each spec section maps to a row; `ok`/`IGNORED`/`FAILED` map to 🟢/🟡/🔴. The table is always current because it is generated from the actual test run, not maintained by hand.

This is the chain: the spec drives the test names → the tests drive the status table → the status table is what users see. Machine-verified, end to end.

### `cargo-nextest`

`cargo-nextest` is the recommended test runner for this project. It produces cleaner output than `cargo test`, supports JUnit XML export for CI, and its test group configuration (`.config/nextest.toml`) allows `cargo nextest run spec` to run all spec coverage tests regardless of which file they eventually live in — though for the reasons above, they live in `spec_coverage.rs`.

Install: `cargo install cargo-nextest`

---

## 7. Known Alternatives Considered

### The Claude Code SDK (Python / TypeScript)

The official Anthropic-maintained SDK for Claude Code. The Python SDK automatically bundles the Claude CLI, provides typed message objects via async iterator, and exposes a native hook system (`UserPromptSubmit`, `Stop`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, and others) that maps almost directly onto `ail`'s pipeline model.

**Why it was not chosen:**

The SDK is the faster path to a v0.0.1 demo. It answers the entire spike phase. If the goal were to validate the concept as quickly as possible, the SDK in Python would be the right choice.

But the SDK is Python or TypeScript. The Node.js runtime baseline is 80–120MB RSS. The strategic case for `ail` rests on being orders of magnitude more memory-efficient than the tooling it orchestrates. An `ail` built on Node would be a heavier process than the pipeline steps it is orchestrating in many configurations — a direct contradiction of the architecture's purpose.

Additionally, the SDK is governed by Anthropic's Commercial Terms of Service, not an open source license. A dependency on a proprietary Anthropic library would create a hard coupling between `ail`'s availability and Anthropic's licensing decisions. For a tool positioned as open infrastructure, this is an unacceptable long-term risk.

The SDK is worth continuing to monitor. If Anthropic publishes a Rust client or a stable, documented protocol specification, those would be worth incorporating.

### PTY Wrapping

Early in the design process, `ail` was expected to need PTY (pseudo-terminal) wrapping to correctly handle interactive CLI tools — the `portable-pty` crate was identified as the likely mechanism. PTY wrapping is the correct approach for tools that only operate interactively and do not expose a programmatic interface.

**Why it was not chosen for v0.0.1:**

The discovery that Claude CLI exposes `--output-format stream-json` and `--input-format stream-json` makes PTY wrapping unnecessary for the Claude CLI runner. The structured JSON interface is strictly better — typed events, unambiguous completion signal, native permission interception — than anything achievable by parsing raw terminal output.

PTY wrapping remains in the architecture as a fallback for Tier 2 runners that do not support structured output. The `portable-pty` crate may still be used for those cases. It is not a primary dependency.

---

*This document will grow as implementation decisions are made. When SPIKE.md is written, findings that affect the architecture described here should be cross-referenced.*
