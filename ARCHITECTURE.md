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
2. [Architectural Principles](#2-architectural-principles)
   - 2.1 Adopt Open Standards First
   - 2.2 15-Factor Application Design
   - 2.3 Agent-First Design
   - 2.4 Separation of UI and Core
   - 2.5 Domain-Driven Design Mindset
   - 2.6 SOLID Principles
   - 2.7 Dependency Injection
   - 2.8 Error Handling — RFC 9457 Inspired
   - 2.9 Observability From Day One
   - 2.10 Testing Strategy
3. [Domain Model](#3-domain-model)
4. [Error Handling](#4-error-handling)
5. [Observability](#5-observability)
6. [The Control Plane / Agent Boundary](#6-the-control-plane--agent-boundary)
7. [High-Level System Model](#7-high-level-system-model)
8. [Runner Adapter Architecture](#8-runner-adapter-architecture)
9. [Stream Parsing Isolation](#9-stream-parsing-isolation)
10. [Testing Strategy](#10-testing-strategy)
11. [Spec Coverage Testing](#11-spec-coverage-testing)
12. [Server Mode & SDK Generation](#12-server-mode--sdk-generation)
13. [Known Alternatives Considered](#13-known-alternatives-considered)

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


## 2. Architectural Principles

These principles govern every significant design decision in `ail`. They are listed here so contributors can reason about tradeoffs consistently rather than relitigating them case by case.

When a decision is ambiguous, ask: which option is more consistent with these principles? If two principles conflict, the one listed earlier takes precedence — though genuine conflicts should be raised as a discussion rather than resolved silently.

---

### 2.1 Adopt Open Standards First

> *Only write our own if it is a domain where we need deep control — a central pillar of the value proposition. The default is to adopt open standards, and the burden of proof is on diverging from them.*

Before designing a solution, search for an existing open standard, RFC, or widely-adopted convention. If one exists and is fit for purpose, use it. Adapt it only where the CLI/runtime context genuinely requires it.

This applies to: error formats (RFC 9457), structured logging (OpenTelemetry), configuration conventions (XDG Base Directory), wire protocols (NDJSON), and API design.

The pipeline language itself — `.ail.yaml` — is the one area where `ail` necessarily defines its own standard. That is the core value proposition. Everything else should defer to prior art.

---

### 2.2 15-Factor Application Design

Inspired by the [12-factor app methodology](https://12factor.net), adapted for a CLI tool and control plane rather than a web service. The spirit of each factor is applied; where a factor is genuinely inapplicable in a CLI context, the nearest meaningful adaptation is noted.

| Factor | Application to `ail` |
|---|---|
| **Codebase** | One repo, one deployable binary. |
| **Dependencies** | All Rust dependencies declared in `Cargo.toml`. No implicit system dependencies beyond the target runner CLI. |
| **Config** | All configuration via `.ail.yaml` and environment variables. No hardcoded values anywhere in the codebase. `ail` never reads config from a location it wasn't told about. |
| **Backing services** | Runners (Claude CLI, Aider, etc.) are treated as attached resources — referenced by configuration, swappable without code changes. |
| **Build, release, run** | `cargo build --release` produces a single static binary. No runtime compilation, no JIT, no interpreter. |
| **Processes** | `ail` is stateless between sessions. Session state lives in explicitly managed session objects, not global mutable state. |
| **Concurrency** | Parallel pipeline steps (planned) are modelled as independent units of work. No shared mutable state between concurrent steps. |
| **Disposability** | Fast startup, clean shutdown. SIGTERM triggers graceful pipeline termination with audit trail flush before exit. |
| **Dev/prod parity** | The same binary runs in development, CI, and production. No "dev mode" that changes behaviour. |
| **Logs** | All output is structured JSON to stdout. The TUI is a rendering layer on top of the same structured events — not a separate code path. *(Adaptation: stdout rather than a log aggregator, because `ail` is a CLI tool.)* |
| **Telemetry** | Structured spans and metrics emitted from day one, even if no exporter is configured. Observability is not an afterthought. |
| **API-first** | Every operation available in the interactive TUI is also available via CLI flags. Agents must be able to drive `ail` without a human present. |
| **Security** | Credentials are never stored by `ail`. All sensitive configuration is via environment variables. The tool runs with the minimum permissions required. |
| **Admin processes** | `ail materialize`, `ail validate`, `ail run` are first-class commands, not afterthoughts. |
| **Port binding** | *Not applicable — `ail` is a CLI tool, not a network service. The nearest adaptation: `ail` does not depend on any ambient service to run. It is self-contained.* |

---

### 2.3 Agent-First Design

> *`ail` is human-focused but must be fully operable by autonomous agents without modification.*

The directive "build infrastructure for agents, not humans" reflects a real architectural constraint: any workflow that works interactively must also work non-interactively. This is not about adding an API later — it is about designing the CLI surface correctly from the start.

**What this requires in practice:**

Every action available in the TUI must have a CLI flag equivalent. A human choosing "Use original prompt" in the HITL preview must be expressible as `--hitl-response use_original`. A human approving a tool call must be expressible as `--auto-approve Read,Glob`.

**Single-turn mode:**

```bash
ail --once "refactor the auth module for DRY compliance"
```

Runs one complete pipeline turn non-interactively. If the pipeline completes without HITL challenges, exits with code 0 and writes the final response to stdout. Suitable for scripting, CI, and autonomous agent use.

**Session keys and resumption:**

When `ail` starts a session it generates a session key:

```bash
ail --session-key abc123   # resume a named session
ail                        # generates and prints a new session key on start
```

If a pipeline run ends on a HITL challenge — a human approval gate that was not pre-authorised — the session key allows an agent to resume:

```bash
ail --resume abc123 --hitl-response "approved: proceed with the refactor"
```

This means a pipeline that encounters an unexpected HITL gate during an autonomous run does not fail silently or block forever. It suspends with a resumable session key that can be passed to a supervising agent or human for resolution.

**Headless mode:**

```bash
ail --headless --once "generate unit tests for src/pipeline.rs"
```

Disables the TUI entirely. All output is structured JSON to stdout. Suitable for programmatic consumption by agents and CI systems. This is distinct from `--dangerously-skip-permissions` — headless mode still enforces all HITL gates, it just expresses them as structured JSON output rather than an interactive TUI prompt.

---

### 2.4 Separation of UI and Core

> *The UI may depend on core. Core must never depend on the UI. This dependency is non-cyclic and enforced at the crate boundary.*

`ail` is structured as a small set of workspace crates:

```
ail-core/     — domain model, pipeline executor, runner adapters, config parsing
ail/          — binary entry point, TUI, CLI argument parsing
ail-init/     — workspace-scaffolding domain crate (SPEC §31); bundles demo/ templates
```

`ail` (the binary) depends on `ail-core` and `ail-init`. `ail-core` has no knowledge that a TUI exists. It communicates via events — the executor emits typed domain events (`StepStarted`, `StepCompleted`, `HitlGateOpened`, etc.); the TUI subscribes to them and renders. The headless mode simply subscribes a different renderer — one that serialises events to JSON.

This boundary is enforced structurally: if code in `ail-core` ever imports from `ail`, that is a build error, not a convention violation. `ail-init` is the pattern for future domain crates — scaffolding (today), a template registry client (planned, HTTP-heavy), and any other concern that does not belong in the core library's dependency graph. Domain crates depend one-way on `ail-core`; the reverse is a compile error.

---

### 2.5 Domain-Driven Design Mindset

`ail` applies DDD as a mindset rather than a strict methodology. The goal is clear domain boundaries, a consistent ubiquitous language, and no leaking of infrastructure concerns into domain logic.

**Ubiquitous language.** The vocabulary in `SPEC.md` is the vocabulary in the code. A `Pipeline` in the spec is a `Pipeline` in the domain model. A `Step` is a `Step`. An `Invocation` is an `Invocation`. When a concept needs a name, the spec is consulted first. If the spec doesn't name it, the name chosen for the code becomes a candidate for the spec.

**The YAML boundary.** Parsed YAML produces DTOs (`PipelineDto`, `StepDto`). These are immediately validated and transformed into domain objects (`Pipeline`, `Step`) at the boundary. Serde structs never become domain objects. The domain model is defined independently of how it is serialised.

```
.ail.yaml → serde (PipelineDto) → validation → Pipeline (domain object)
                                       ↑
                              errors here are user-facing
                              parse errors with span information
```

**Bounded contexts.** Three clear contexts exist in `ail`:

- **Pipeline context** — the domain model: what a pipeline is, what steps are, how inheritance works, how conditions evaluate.
- **Execution context** — the runtime: how a pipeline run is managed, how steps are sequenced, how HITL gates suspend and resume.
- **Runner context** — the adapter layer: how a specific CLI tool is invoked, how its output is parsed, how permissions are negotiated.

These contexts communicate through defined interfaces, not by sharing internal types.

**Name things for their actual role.** Variable and type names should accurately describe what a thing *is*, not what it happens to be implemented as. A log of what happened during a session is not `events` (which implies event-driven architecture or observability traces) — it is a `TurnLog` or `SessionHistory`. When choosing a name, ask: would a new contributor, reading this name in isolation, understand what it represents? If there is a more precise word, use it.

---

### 2.6 SOLID Principles

Applied pragmatically in Rust. Rust's trait system maps cleanly onto several SOLID principles.

**Single Responsibility.** Each module, struct, and function has one reason to change. The config parser changes when the YAML format changes. The condition evaluator changes when condition semantics change. They never change for the same reason.

**Open/Closed.** New runners are added by implementing the `Runner` trait, not by modifying the pipeline executor. New step types are added at the dispatch boundary, not by adding branches inside existing step logic.

**Liskov Substitution.** Any `Runner` implementation must be substitutable for any other without the pipeline executor knowing the difference. A test double that returns canned responses must be a valid `Runner`.

**Interface Segregation.** Traits are narrow. A runner that only needs to support single-turn invocation should not be forced to implement session resumption. Capabilities are declared separately and checked at runtime.

**Dependency Inversion.** The pipeline executor depends on the `Runner` trait, not on `ClaudeCliRunner`. Config loading depends on a `ConfigSource` trait, not on the filesystem directly. This makes the executor testable without spawning real processes.

---

### 2.7 Dependency Injection

Dependencies are injected, not constructed internally. The pipeline executor receives a `Runner` implementation. The TUI renderer receives an event stream. Nothing reaches for a global or constructs its own dependencies.

In Rust this is expressed through trait objects and function parameters rather than a DI container. The effect is the same: callsites receive their dependencies from outside, which makes them testable in isolation and makes the dependency graph explicit and auditable.

---

### 2.8 Error Handling — RFC 9457 Inspired

`ail` models errors as structured values, inspired by **RFC 9457** (Problem Details for HTTP APIs), adapted for a CLI and Rust context.

Every error in `ail` has:

- **`error_type`** — a stable, namespaced string identifying the error class. Functions as a stable identifier for documentation links and programmatic handling. Example: `ail:pipeline/step-timeout`.
- **`title`** — a short, human-readable summary. Does not change between occurrences of the same error type. Example: `"Step execution timed out"`.
- **`detail`** — a specific, instance-level description. May include variable context. Example: `"Step 'security_audit' exceeded timeout_seconds: 30 in pipeline run abc123"`.
- **`context`** — optional structured data relevant to the error: step ID, pipeline run ID, provider name, etc. Machine-readable for agent consumption.

```rust
pub struct AilError {
    pub error_type: &'static str,        // "ail:config/invalid-yaml"
    pub title: &'static str,             // "Pipeline configuration is invalid"
    pub detail: String,                  // specific instance description
    pub context: Option<ErrorContext>,   // structured supplementary data
}

pub struct ErrorContext {
    pub pipeline_run_id: Option<RunId>,
    pub step_id: Option<StepId>,
    pub source: Option<Box<dyn std::error::Error>>,
    // ...extensible
}
```

`panic` and `unwrap` are not permitted in production code paths. Every failure mode is a typed `Result<T, AilError>`. Panics are reserved for invariant violations that represent programming errors — not user errors, not runner errors, not configuration errors.

Errors that reach the user — whether via TUI or structured JSON in headless mode — are rendered from the same `AilError` type. The rendering layer formats it appropriately for the context; the error itself is format-agnostic.

---

### 2.9 Observability From Day One

Structured observability is wired in before any significant logic is written. It costs almost nothing to add at the start and is extremely expensive to retrofit.

**Logging.** `tracing` crate (the Rust ecosystem standard). All log statements use `tracing::info!`, `tracing::warn!`, `tracing::error!` with structured fields — never `println!` or `eprintln!`. Log statements in production code that use string formatting instead of structured fields are a code smell.

**Spans.** Every pipeline run is a span. Every step execution is a child span. Every runner invocation is a child span of the step. Span attributes carry the ubiquitous language: `ail.step.id`, `ail.pipeline.run_id`, `ail.runner.name`, `ail.step.cost_usd`.

**Exporters.** In v0.0.1, the only exporter is stdout JSON (15-factor compliant). The OTEL exporter described in SPEC §22 is wired to the same span infrastructure — adding it is a configuration change, not a code change.

**No silent failures.** Every error is logged at the appropriate level before being propagated or handled. A pipeline that silently swallows an error is a bug.

---

### 2.10 Testing Strategy

Three layers. Each layer tests different things. Each layer is necessary; none is sufficient alone.

**Layer 1 — Unit tests (in `src/`)**
Pure functions, no I/O. Domain logic: condition evaluation, template variable resolution, `on_result` matching, YAML-to-domain transformation, error formatting. These tests run in milliseconds. They live in `#[cfg(test)]` modules in the same file as the code they test.

```rust
// src/pipeline/condition.rs
#[cfg(test)]
mod tests {
    #[test]
    fn condition_always_evaluates_true() { ... }

    #[test]
    fn condition_never_evaluates_false() { ... }

    #[test]
    fn condition_if_code_changed_detects_code_block() { ... }
}
```

**Layer 2 — Integration tests (in `tests/`)**
Real I/O, real subprocess invocations, real CLI interactions where possible. Runner adapter tests invoke the actual Claude CLI. Config tests read real YAML files from `tests/fixtures/`. These tests are slower and may require credentials — they are gated in CI accordingly.

**Layer 3 — Spec coverage tests (`tests/spec_coverage.rs`)**
The acceptance layer. Each test corresponds to a named feature in SPEC.md, organised by section number. These tests operate end-to-end: construct YAML, run through the full pipeline, assert observable behaviour. They are the definitive answer to "does `ail` implement what the spec says?" See §11 for full detail.

**Test doubles.** The `Runner` trait boundary enables test doubles without mocking frameworks. A `StubRunner` that returns canned responses is a valid `Runner` implementation. Integration tests use real runners; unit and spec coverage tests use stubs where appropriate.

**The testing pyramid in summary:**

```
        ╱ spec_coverage ╲      ← spec acceptance (tests/spec_coverage.rs)
       ╱   integration   ╲     ← real I/O, real CLI (tests/)
      ╱      unit         ╲    ← pure functions, no I/O (src/**/#[cfg(test)])
```

---

## 3. Domain Model

The domain model is defined independently of how it is serialised (YAML) or how it is rendered (TUI, JSON). Serde structs are DTOs — they cross the boundary from YAML to domain objects through a validation step that produces typed errors.

### Core Types

```
Pipeline
  └── meta: PipelineMeta
  └── providers: Map<ProviderId, ProviderString>
  └── defaults: StepDefaults
  └── steps: Vec<Step>
  └── from: Option<FilePath>        ← resolved at load time

Step
  └── id: StepId                    ← newtype, not String
  └── body: StepBody                ← enum: Prompt | Skill | SubPipeline | Action
  └── provider: Option<ProviderId>
  └── condition: Option<Condition>
  └── on_result: Option<OnResult>
  └── on_error: OnError
  └── tools: ToolPolicy
  └── before: Vec<ChainStep>        ← private pre-processing
  └── then: Vec<ChainStep>          ← private post-processing
  └── sealed: bool

Invocation
  └── prompt: PromptText            ← what the human typed
  └── response: Option<ResponseText> ← runner's reply; None until complete

PipelineRun
  └── run_id: RunId                 ← generated on start; used for session resumption
  └── pipeline: Pipeline
  └── invocation: Invocation
  └── turn_log: TurnLog             ← ordered record of what happened this run
  └── session_allowlist: ToolAllowlist ← in-memory; not persisted

TurnLog                             ← the ordered history of a pipeline run
  └── entries: Vec<TurnEntry>       ← each step's prompt, response, events
```

### Naming Notes

`TurnLog` is used instead of `events` (which implies event-driven architecture or observability traces) and instead of `history` (which implies persistence across sessions). A turn log is the record of what happened in this run — ordered, bounded, and scoped to a single `PipelineRun`. The name is borrowed from conversational AI terminology where a "turn" is one exchange.

`StepId`, `RunId`, `ProviderId`, `PromptText`, `ResponseText` are newtypes over `String` or `Uuid`. They prevent the class of bugs where a step ID is passed where a run ID is expected. The Rust type system enforces this at compile time.

---

## 4. Error Handling

See §2.8 (Architectural Principles) for the philosophy. This section records implementation conventions.

**Error crate.** All `AilError` types are defined in `ail-core::error`. No other module defines its own error enum without wrapping `AilError`. Error type strings follow the pattern `ail:<context>/<specific>`, e.g. `ail:config/missing-version`, `ail:pipeline/step-timeout`, `ail:runner/unexpected-exit`.

**The `?` operator.** Used freely for propagation. Every function that can fail returns `Result<T, AilError>`. The conversion from underlying errors (serde, IO, etc.) happens at the boundary where the error is first encountered — not deep in the call stack.

**User-facing errors.** Errors that reach the user are rendered by the display layer (`ail::display::error`). The same `AilError` renders as formatted terminal output in TUI mode and as structured JSON in headless mode. The error type is never rewritten for display — only formatted.

**Panic policy.** `unwrap()` and `expect()` are permitted only in:
- Test code
- Cases where the invariant is genuinely guaranteed by the type system (document why with a comment)

All other uses are a code review failure.

---

## 5. Observability

See §2.9 (Architectural Principles) for the philosophy. This section records implementation conventions.

**Crate:** `tracing` for spans and structured logging. `tracing-subscriber` for configuring output. No `log` crate — `tracing` supersedes it.

**Span naming convention:** `ail.<subsystem>.<operation>`. Examples: `ail.pipeline.run`, `ail.step.execute`, `ail.runner.invoke`. Span names are stable — they appear in exported traces and changing them is a breaking change for anyone with dashboards built on them.

**Field naming convention:** snake_case, namespaced to `ail.*` for fields specific to `ail`. Standard OTEL semantic convention fields (e.g. `service.name`) use the OTEL names without modification.

**Log levels:**
- `ERROR` — something failed that the user needs to know about
- `WARN` — something unexpected happened but execution continued; the user may want to know
- `INFO` — significant lifecycle events (pipeline started, step completed, session ended)
- `DEBUG` — developer-relevant detail (condition evaluated, template resolved, runner invoked)
- `TRACE` — verbose internal state; NDJSON event parsing, byte-level details

Production builds default to `INFO`. Debug builds default to `DEBUG`. `TRACE` must be explicitly enabled.

---

## 6. The Control Plane / Agent Boundary

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

## 7. High-Level System Model

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

## 8. Runner Adapter Architecture

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

**Tier 4 — Native REST runner (planned):**
A `NativeRestRunner` that implements the `Runner` trait by calling an OpenAI-compatible `/v1/chat/completions` REST endpoint directly — no CLI intermediary required. This enables `ail` to orchestrate any model host that exposes the OpenAI-compatible API: Ollama, Together AI, Groq, LiteLLM, corporate LLM proxies, and Anthropic's own compatibility layer.

This is a meaningful architectural expansion: rather than wrapping an agent that calls an LLM, `ail` becomes the agent for that turn. The `NativeRestRunner` must therefore also manage conversation history, tool call parsing and dispatch, streaming delta reassembly, and context window state — concerns that CLI-backed runners delegate to the underlying tool.

The `Runner` trait boundary is designed to accommodate this. When built, `NativeRestRunner` will live in `ail-core` behind a Cargo feature flag (`--features native-runner`) to keep the default binary footprint lean for users who don't need it.

**Trigger for building this:** a concrete use case that cannot be served by any existing CLI runner. Do not build speculatively.

---

## 9. Stream Parsing Isolation

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

## 10. Testing Strategy

Testing in `ail` operates at three layers. Each tests different concerns. All three are necessary; none is sufficient alone. The testing pyramid is described in §2.10 (Architectural Principles). This section records conventions and tooling.

**Tooling:** `cargo-nextest` is the required test runner. Install with `cargo install cargo-nextest`. All CI runs use `cargo nextest run`.

**Unit tests** live in `#[cfg(test)]` modules inside `src/`. They test pure domain logic with no I/O. They run in milliseconds and must pass without any external dependencies.

**Integration tests** live in `tests/` (excluding `spec_coverage.rs`). They may invoke real CLI tools and read real files. Tests that require credentials or external services are gated with `#[ignore]` by default and run explicitly in CI with the appropriate environment variables set.

**Fixture files** live in `tests/fixtures/`. YAML pipeline files used by multiple tests live here. Test fixtures are part of the codebase and reviewed with the same care as production code — a misleading fixture is as harmful as misleading test logic.

**Test doubles** implement the `Runner` trait. A `StubRunner` that returns configurable canned responses is provided in `ail-core` under `#[cfg(test)]`. It is not a mock framework — it is a real `Runner` implementation that returns what you tell it to.

---

## 11. Spec Coverage Testing

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


> **Status: Planned.** Not part of v0.0.1 or v0.1. The crate boundary established now (`ail-core` / `ail` / `ail-server`) ensures this costs almost nothing to add when the time comes. See `API.md` for the endpoint surface design.

### The Vision

`ail` is a CLI tool. It is also, optionally, an HTTP server. The same `ail-core` that powers the interactive TUI and headless mode also powers an HTTP API — pipelines execute the same way regardless of how they were invoked. The server is a frontend, not a separate product.

This enables three things that would otherwise require bespoke integration work:

**1. Auto-generated native clients in any language.**
An OpenAPI 3.1 spec, generated directly from the server's handler code, feeds standard generators. A Python developer, a C# developer, a TypeScript developer — each runs one command and gets a fully typed, idiomatic client for their language. No hand-written wrappers. No documentation drift. The spec *is* the contract.

**2. A web UI with no custom frontend work.**
Swagger UI, embedded in `ail serve`, available at `http://localhost:PORT/ui` by default. Functional, navigable, explorable — for free. A purpose-built pipeline visualisation UI is a later roadmap item; Swagger UI is the zero-cost first step.

**3. Seamless agent integration.**
An autonomous agent that wants to drive `ail` pipelines as part of a larger workflow calls the same HTTP API a human would use via the web UI. The SSE stream endpoint gives the agent real-time pipeline events. The HITL endpoint lets a supervising agent respond to gates programmatically. This is agent-first design (§2.3) expressed at the API layer.

---

### Crate Structure

```
ail-core/        — domain model, pipeline executor, runner adapters
                   no knowledge of HTTP, TUI, or CLI flags
                   this is the only crate that matters for correctness

ail/             — the CLI binary
                   depends on ail-core
                   owns: TUI, CLI argument parsing, interactive session loop

ail-server/      — the server binary (or feature-flagged crate)
                   depends on ail-core
                   owns: HTTP handlers, SSE streaming, OpenAPI spec generation
                   crates: axum (HTTP), utoipa (OpenAPI), tokio-stream (SSE)
```

`ail-core` never imports from `ail` or `ail-server`. This is enforced at the crate boundary — a circular import is a compile error. Both frontends are equal consumers of the same core.

---

### API Surface

The full design lives in `API.md`. The shape at a glance:

```
POST   /sessions                      start a new session
GET    /sessions/{key}                get session state
DELETE /sessions/{key}                end a session

POST   /sessions/{key}/turns          submit a prompt; runs the full pipeline
GET    /sessions/{key}/turns/{id}     get a completed turn's full detail

GET    /sessions/{key}/stream         SSE — real-time pipeline event stream
                                      same typed events the TUI subscribes to
                                      browser-consumable, agent-consumable

POST   /sessions/{key}/hitl/{gate_id} respond to a pending HITL gate
GET    /sessions/{key}/hitl/pending   list gates waiting for a response

POST   /pipelines/validate            validate a .ail.yaml without running it
POST   /pipelines/materialize         materialize via API

GET    /openapi.json                  the OpenAPI 3.1 spec (auto-generated)
GET    /ui                            Swagger UI (embedded, zero config)
```

---

### OpenAPI Generation

`utoipa` derives the OpenAPI spec from handler annotations at compile time. The spec is always current — it cannot drift from the implementation because it is generated from it.

```rust
// Example handler annotation
#[utoipa::path(
    post,
    path = "/sessions/{key}/turns",
    request_body = CreateTurnRequest,
    responses(
        (status = 201, description = "Turn created", body = TurnResponse),
        (status = 409, description = "HITL gate pending", body = AilError),
    ),
    tag = "turns"
)]
async fn create_turn(/* ... */) -> impl IntoResponse { /* ... */ }
```

The spec is served at `/openapi.json` and can be dumped to disk:

```bash
ail serve --openapi-only > ail-openapi.json
```

---

### SDK Generation Workflow

With the spec on disk or at a URL, any OpenAPI generator produces a native client:

```bash
# Python
openapi-generator generate -i ail-openapi.json -g python -o ./clients/python

# TypeScript / Node
openapi-generator generate -i ail-openapi.json -g typescript-fetch -o ./clients/typescript

# C#
openapi-generator generate -i ail-openapi.json -g csharp -o ./clients/csharp

# Go, Java, Ruby, Rust, Kotlin... — all supported by openapi-generator
```

Or pulling from a hosted spec:

```bash
openapi-generator generate   -i https://api.ail.sh/openapi.json   -g python   -o ./ail-client
```

A generated Python client looks like this in practice:

```python
from ail_client import AilClient

client = AilClient(base_url="http://localhost:7823")

# Start a session
session = client.sessions.create(
    pipeline_path="./quality-gates.ail.yaml"
)

# Submit a prompt and run the pipeline
turn = client.turns.create(
    session_key=session.key,
    prompt="refactor the authentication module for DRY compliance"
)

# The full pipeline ran. Access the result.
print(turn.invocation.response)   # what the agent produced
print(turn.steps[0].response)     # what the first pipeline step produced
print(turn.cost_usd)              # total cost for this turn
```

The client is typed, idiomatic, and handles the SSE stream, authentication, retries, and error deserialisation automatically.

---

### The SSE Stream

The `/sessions/{key}/stream` endpoint emits the same `RunnerEvent` types that the TUI subscribes to, serialised as JSON-over-SSE. A browser or agent subscribes and receives live pipeline progress:

```
event: step_started
data: {"step_id": "security_audit", "pipeline_run_id": "abc123"}

event: text_delta
data: {"step_id": "security_audit", "text": "Reviewing for vulnerabilities..."}

event: hitl_gate_opened
data: {"gate_id": "xyz", "step_id": "security_audit", "message": "Findings require review"}

event: step_completed
data: {"step_id": "security_audit", "response": "...", "cost_usd": 0.002}

event: pipeline_completed
data: {"run_id": "abc123", "total_cost_usd": 0.008, "duration_ms": 4201}
```

The web UI is a browser that subscribes to this stream and renders it. A purpose-built pipeline visualisation UI — showing the step sequence, real-time streaming output, HITL gates as modal dialogs, the turn log as a timeline — is the natural next step after Swagger UI. It requires no new API design; only frontend work.

---

### Sequencing

| Milestone | What ships |
|---|---|
| v0.0.1 | CLI only. `ail-core` crate boundary established. No server code. |
| v0.1 | Stable pipeline execution. `ail-core` API considered stable enough to build on. |
| v0.2 | `ail serve` ships. OpenAPI spec generated. Swagger UI embedded. |
| v0.3 | First-party generated clients published: Python, TypeScript. Community generators for others. |
| Later | Purpose-built web UI. Hosted spec at `api.ail.sh`. |

The critical decision that must be made now — before v0.0.1 ships — is the `ail-core` crate boundary. Everything else follows from it.

---

## 12. Server Mode and SDK Generation

> *"Any workflow that works interactively must also work non-interactively."*
> — §2.3 Agent-First Design

`ail serve` is a first-class operating mode alongside the interactive TUI and `--headless` non-interactive mode. It exposes `ail-core`'s full pipeline execution capability as an HTTP API, described by an OpenAPI 3.1 specification, enabling auto-generated native clients in any language and a hostable web UI.

This is not a bolt-on. It is the natural conclusion of the architectural decisions already made: `ail-core` has no knowledge of how it is driven, the UI/Core separation is enforced at the crate boundary, and every operation available interactively is also available programmatically. The server is simply the third frontend — after the TUI and headless mode — subscribing to the same typed domain events from the same `ail-core` executor.

---

### 12.1 Crate Structure

```
ail-core/          — domain model, pipeline executor, runner adapters
                     no knowledge of TUI, HTTP, or CLI argument parsing
                     this is the heart; everything else is a frontend

ail/               — interactive binary
                     TUI (ratatui), CLI argument parsing (clap)
                     depends on ail-core

ail-server/        — server binary (or optional feature flag on ail)
                     HTTP server (axum), SSE, OpenAPI spec (utoipa)
                     depends on ail-core
                     no dependency on ail (the TUI binary)
```

`ail-server` and `ail` are siblings — both thin frontends over `ail-core`. They share no code with each other. Adding `ail-server` requires no changes to `ail-core` and no changes to the interactive `ail` binary. The crate boundary established in §2.4 makes this addition essentially free once the core is stable.

---

### 12.2 Operating `ail serve`

```bash
# Start the server on the default port
ail serve

# Specify port and bind address
ail serve --port 7823 --bind 0.0.0.0

# Dump the OpenAPI spec and exit (no server started)
ail serve --openapi > ail-openapi.json

# Serve with a specific pipeline as the default
ail serve --pipeline ./team-pipeline.ail.yaml
```

On startup, `ail serve` prints:

```
ail server listening on http://localhost:7823
  API:  http://localhost:7823/api/v1
  UI:   http://localhost:7823/ui
  Spec: http://localhost:7823/api/v1/openapi.json
```

The web UI is available immediately at `/ui` — Swagger UI embedded in the binary, no separate installation required. It is functional as a developer tool and API explorer from day one.

---

### 12.3 API Surface

The API is organised around sessions and turns — the same mental model as the interactive `ail`. A session wraps a pipeline and a runner. A turn is one human prompt and its complete pipeline execution.

```
Sessions
  POST   /api/v1/sessions                     start a session
  GET    /api/v1/sessions/{key}               get session state
  DELETE /api/v1/sessions/{key}               end a session

Turns
  POST   /api/v1/sessions/{key}/turns         submit a prompt; runs the full pipeline
  GET    /api/v1/sessions/{key}/turns         list completed turns
  GET    /api/v1/sessions/{key}/turns/{id}    get a specific turn with full detail

Streaming
  GET    /api/v1/sessions/{key}/stream        SSE stream of live pipeline events

HITL
  GET    /api/v1/sessions/{key}/hitl/pending  list pending human-in-the-loop gates
  POST   /api/v1/sessions/{key}/hitl/{gate}   respond to a HITL gate

Pipelines
  POST   /api/v1/pipelines/validate           validate a .ail.yaml without running
  POST   /api/v1/pipelines/materialize        materialize via API

Meta
  GET    /api/v1/health                       health check
  GET    /api/v1/openapi.json                 OpenAPI 3.1 spec (self-describing)
  GET    /ui                                  embedded Swagger UI
```

See `API.md` for the full endpoint specification including request/response schemas, error codes, and example payloads.

---

### 12.4 The SSE Stream

The Server-Sent Events stream at `/api/v1/sessions/{key}/stream` is the real-time channel. It emits the same typed domain events the TUI subscribes to, serialised as JSON:

```
event: step.started
data: {"step_id": "security_audit", "run_id": "abc123", "timestamp": "..."}

event: step.text_delta
data: {"step_id": "security_audit", "delta": "Reviewing authentication..."}

event: step.completed
data: {"step_id": "security_audit", "response": "...", "cost_usd": 0.003}

event: hitl.gate_opened
data: {"gate_id": "xyz", "gate_type": "tool_permission", "tool": "WebFetch", "details": {...}}

event: pipeline.completed
data: {"run_id": "abc123", "total_cost_usd": 0.021, "duration_ms": 4823}
```

The web UI is a browser client subscribed to this stream. A Python SDK wraps it in an async iterator. A C# SDK exposes it as an `IAsyncEnumerable<PipelineEvent>`. The stream is the same in every case — the rendering and consumption differ.

This architecture means the web UI is never a separate code path from the CLI. Both consume identical events from `ail-core`. A bug fixed in the event model is fixed everywhere simultaneously.

---

### 12.5 OpenAPI 3.1 and SDK Generation

The OpenAPI spec is generated from the server code, not maintained by hand. The `utoipa` crate derives the spec from annotated Rust handler functions:

```rust
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{key}/turns",
    request_body = CreateTurnRequest,
    responses(
        (status = 201, description = "Turn created", body = TurnResponse),
        (status = 404, description = "Session not found", body = AilProblemDetail),
        (status = 409, description = "HITL gate pending", body = AilProblemDetail),
    )
)]
async fn create_turn(/* ... */) { /* ... */ }
```

The spec is always current because it is derived from the implementation. There is no separate YAML file to keep in sync. `ail serve --openapi` dumps it; CI validates it hasn't changed unexpectedly.

**Generating clients:**

```bash
# Dump the spec
ail serve --openapi > ail-openapi.json

# Generate a Python client
openapi-generator generate -i ail-openapi.json -g python -o ./sdk/python

# Generate a TypeScript client
openapi-generator generate -i ail-openapi.json -g typescript-fetch -o ./sdk/typescript

# Generate a C# client
openapi-generator generate -i ail-openapi.json -g csharp -o ./sdk/csharp
```

Or pull from a hosted spec and generate without cloning the repo:

```bash
openapi-generator generate \
  -i https://api.ail.sh/openapi.json \
  -g python \
  -o ./my-app/ail_client
```

The generated clients handle authentication, serialisation, retries, and the SSE stream. A Python developer integrating `ail` into their own application writes:

```python
from ail_client import AilClient, CreateSessionRequest

client = AilClient(base_url="http://localhost:7823")

session = client.sessions.create(CreateSessionRequest(
    pipeline_path="./my-pipeline.ail.yaml"
))

turn = session.turns.create(prompt="refactor the auth module for DRY compliance")

for event in turn.stream():
    print(event)

print(turn.response)
```

This is a typed, idiomatic Python client — not a thin HTTP wrapper, not curl in a trenchcoat. The same quality of experience is available in every language OpenAPI Generator supports.

---

### 12.6 The Web UI

**Phase 1 — Swagger UI (essentially free).**
Embedded in `ail serve` from the first server release. Available at `/ui`. Provides a functional interface for API exploration, manual pipeline execution, and HITL gate responses. Requires zero custom frontend work.

**Phase 2 — Purpose-built pipeline visualiser (roadmap).**
A dedicated web interface that renders the pipeline execution as a visual timeline: step sequence, which steps ran, what each produced, HITL gates as modal dialogs, cost per step, the turn log as a structured history. Built as a browser client subscribed to the SSE stream. The architecture already supports this — it is the same event subscription model as everything else, just rendered in a browser instead of a terminal.

Phase 2 is a meaningful product investment but not a prerequisite for the server mode to be useful. The Swagger UI in Phase 1 makes `ail serve` immediately usable for developers and agents. Phase 2 makes it accessible to non-CLI users and teams who prefer a visual interface.

---

### 12.7 Hosting and Deployment

`ail serve` is designed to be deployable by a single `docker run`:

```bash
docker run -p 7823:7823 \
  -v $(pwd)/pipelines:/pipelines \
  -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \
  ghcr.io/ail-sh/ail serve --pipeline /pipelines/team.ail.yaml
```

The Docker image is small — the Rust binary is statically linked with no runtime dependencies. The image is built `FROM scratch` with only the binary and a TLS certificate bundle. Target image size: under 20MB.

For teams, `ail serve` behind a reverse proxy (nginx, Caddy, Cloudflare Tunnel) provides shared pipeline infrastructure with no per-developer installation required. Pipeline files are mounted as volumes. The same `.ail.yaml` that a developer runs locally is the one the server runs — no translation layer, no separate deployment configuration.

---

### 12.8 Sequencing

`ail serve` is not a v0.0.1 deliverable. The correct sequence is:

1. **v0.0.1** — Establish `ail-core` / `ail` crate separation correctly. The server mode costs almost nothing to add later if the boundary is clean now.
2. **v0.1** — Stabilise the domain model and the `Runner` trait. Spec coverage at 80%+.
3. **v0.2** — `ail serve` with Phase 1 web UI (Swagger UI). OpenAPI spec published. Basic SDK generation documented.
4. **v0.3+** — Hosted spec at `api.ail.sh`. Official SDKs for Python, TypeScript, and one compiled language (Go or C#). Phase 2 web UI begins.

The decision to sequence this correctly — rather than building it immediately — is itself an application of §2.1 (Adopt Open Standards First, earn complexity): `ail-core` must be stable before the API surface is frozen, because the API is a public contract and changing it breaks generated clients.

---


## 13. Known Alternatives Considered

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
