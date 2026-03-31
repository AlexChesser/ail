# ail — Alexander's Impressive Loops (AI Loops)

> **The executive function layer LLM agents structurally lack.**

[![Spec: CC BY-SA 4.0](https://img.shields.io/badge/spec-CC%20BY--SA%204.0-lightgrey.svg)](#license)
[![Core: MPL 2.0](https://img.shields.io/badge/core-MPL%202.0-blue.svg)](#license)
[![CLI: AGPL v3](https://img.shields.io/badge/cli-AGPL%20v3-red.svg)](#license)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Active Development](https://img.shields.io/badge/status-active%20development-yellow.svg)](#roadmap)

`ail` is an open-source pipeline runtime that wraps AI coding agents like the Claude CLI and automatically runs a deterministic chain of follow-up steps after every agent response — before control ever returns to the human.

> ⚠️ **This project is in early development.** The parser and domain model are working, but the executor is not yet complete. The pipeline language is a working hypothesis — it feels solid, but real-world implementation will test that. The examples below show what `ail` is being built toward — not what it does today. See [Current Status](#current-status) for what is and isn't implemented.

---

> TODO: add a **QUICKSTART** section for people who DGAF about the cognitive and neuroscience 🤣

---

## Your Agent Has a Diagnosis

Cognitive science has been studying these failure modes since Harlow published *Passage of an Iron Bar through the Head* in 1848. By 1986, Alan Baddeley had a name for the cluster: **Dysexecutive Syndrome** — the predictable behavioral profile of a system with capable reasoning and absent executive control.

Your agent's failures aren't random. They have clinical names:

| Failure | Clinical Name | What You See |
|---|---|---|
| Repeats the same tool call past the point it was working | **Perseveration** | The agent planes the board through to the bench |
| Implements the schema you asked a question about | **Goal Substitution** | You asked for a discussion; it got to work |
| Cites a function that doesn't exist | **Source Monitoring Failure** | Can't distinguish what it read from what it generated |
| Reports no issues with code that has obvious problems | **Anosognosia** | High confidence is a syntactic property of the output |

The standard response is to write more instructions — refine `CLAUDE.md`, add more skills, be more explicit. But an instruction inside a context window is subject to everything else in that window. Sessions grow. Tool calls accumulate. Earlier instructions drift toward the middle where the attention mechanism is weakest. Liu et al. documented this as the *lost-in-the-middle effect* in 2024; the Chroma Research team confirmed it across 18 frontier models in 2025. The carefully written rule becomes one voice in a crowd.

`ail` moves the behavior out of the context entirely. A pipeline step fires because it was declared, not because the model remembered to do it.

---

## The Treatment

Diamond's 2013 synthesis of executive function research identified three components that don't emerge from capability — they have to be built:

| Executive Component | Agent Failure It Addresses | `ail` Primitive |
|---|---|---|
| **Inhibitory control** — suppress dominant but wrong responses | Perseveration | `max_retries:` + `on_error: abort_pipeline` |
| **Working memory updating** — hold task-relevant state, release what's stale | Goal substitution, context contamination | Pipeline run log + `{{ step.<id>.response }}` |
| **Cognitive flexibility** — shift strategy when the current one stops working | Perseveration, misapplied context | `on_result:` branches, conditional `pipeline:` steps |

And the fourth, from Anokhin's 1955 work on feedback circuits: the **action acceptor** — a comparison of what you intended against what you actually produced. When this mechanism is damaged, output is generated without any internal signal that something went wrong.

Every `on_result:` block is an action acceptor. The pipeline run log is what makes it honest: the intended prompt and the actual response are persisted independently, before the acceptor step runs, so it can't be contaminated by the output it's evaluating.

```yaml
version: "0.1"

pipeline:
  - id: action_acceptor
    prompt: |
      Original request: {{ step.invocation.prompt }}
      Result produced: {{ step.invocation.response }}
      Does the result achieve what was requested?
      Answer ACHIEVED or MISMATCH. One word only.
    on_result:
      contains: "ACHIEVED"
      if_true:
        action: break
      if_false:
        action: pause_for_human
        message: "Action acceptor detected a mismatch between intent and output."
```

---

## The Core Guarantee

> For every completion event produced by an underlying agent, `ail` will begin executing the pipeline defined in the active `.ail.yaml` file before control returns to the human.

Steps execute in order. Individual steps may be skipped by declared conditions or disabled explicitly. Execution may terminate early via `break`, `abort_pipeline`, or an unhandled error. All of these are explicit, declared outcomes — not silent failures.

*This is the guarantee the project is being built toward. Whether the design as specified actually delivers it cleanly is what implementation will tell us.*

---

## The Pipeline Language

`ail` pipelines are declared in a `.ail.yaml` file. The examples below show the intended syntax as currently designed.

### The Simplest Possible Pipeline

```yaml
# .ail.yaml
version: "0.1"

pipeline:
  - id: review
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

### A Quality Loop

```yaml
version: "0.1"

meta:
  name: "Personal Quality Gates"

defaults:
  provider: openai/gpt-4o-mini
  on_error: pause_for_human

pipeline:
  - id: dry_refactor
    condition: if_code_changed
    prompt: "Refactor the code above to eliminate unnecessary repetition."

  - id: test_writer
    condition: if_code_changed
    prompt: "Write unit tests for any new functions in the code above."

  - id: security_audit
    provider: anthropic/claude-opus-4-5
    condition: if_code_changed
    prompt: "Review the changes for common security vulnerabilities. If none, respond SECURITY_CLEAN."
    on_result:
      contains: "SECURITY_CLEAN"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Security issues detected. Review before proceeding."
```

### The Two Layers

`ail` is built around a strict two-layer model that must never be confused:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable. Adding more skill instructions to a context saturation problem just makes a larger context. `ail` operates at the layer that decides what goes into the context at all.

Full language documentation is in [`spec/README.md`](spec/README.md).

---

## Scope Discipline

The compass for every implementation decision is: *does this serve the frontal lobe?*

A feature belongs in `ail` if it:
- **Addresses one of the four failure modes** — perseveration, goal substitution, source monitoring failure, or anosognosia; or
- **Strengthens one of Diamond's three executive function components** — inhibitory control, working memory updating, or cognitive flexibility; or
- **Extends `ail`'s capacity to select, compose, or improve its own pipelines** — the supervisory layer that decides *which* script to run, not just *that* it runs.

> *** TODO: consider the introduction of *ANY* currently known cortical or executive function here as well.  Specifically planning like from Anderson's "**Adaptive Control of Thought—Rational**" (ACT-R) the cognitive architecture or others lik Piaget & Schemas.  Or delving across into the Educational research angle (referecnes in older planning documents - will pull up laterelsewhere). 

Features that serve general task execution without mapping to any of these three categories belong to the agent layer beneath `ail`, not to the control plane above it.

---

## Current Status

The project is being built spec-first. The spec represents a hypothesis — things that sound good on paper may turn out to be awkward in practice. The spec will change as reality pushes back.

```bash
cargo nextest run --no-fail-fast --run-ignored all
```

Current result: **64 passing, 13 failing** across 77 tests.

### What works today

- **Config parsing and validation** — `.ail.yaml` files parse correctly to domain types; validation errors are structured and informative
- **File discovery** — the full resolution order (explicit path → `.ail.yaml` → `.ail/default.yaml` → `~/.config/ail/default.yaml`) is implemented
- **Domain model** — `Pipeline`, `Step`, `PipelineRun`, `TurnLog`, and associated newtypes are implemented and correct
- **Session and run identity** — `RunId` generation, session state, turn log append and ordering
- **Step sequencing** — steps execute in declaration order; the executor runs a passthrough pipeline end-to-end
- **Step field validation** — duplicate IDs, missing primary fields, misplaced `invocation` step are all caught at parse time
- **Claude CLI runner** — the runner adapter exists and communicates with the Claude CLI via `--output-format stream-json`

### What doesn't work yet

- **`on_result` branching** — `contains` matching, `continue`, `pause_for_human`, `break`, and `abort_pipeline` are not yet wired up. This is the core value proposition and the immediate next focus.
- **Conditions** — `if_code_changed`, `always`, `never` and other conditional skip logic are not yet evaluated
- **Pipeline inheritance (`FROM`)** — parsing and `materialize` traversal are not yet implemented
- **Skills** — the `skill:` step type is not yet implemented
- **Provider/model routing** — the `provider:` override on individual steps is not yet passed to the runner

---

## How It Works

`ail` operates as a thin control plane sitting between the human and the underlying AI agent:

```
Human prompt
    ↓
ail (control plane)
    ├── YAML parser (.ail.yaml)
    ├── Pipeline executor
    │     ├── step sequencing        ✓ implemented
    │     ├── condition evaluation   ✗ not yet
    │     ├── on_result branching    ✗ not yet
    │     ├── HITL gate management   ✗ not yet
    │     └── template variable resolution
    └── TUI (terminal UI)
            ↓  stdin/stdout (NDJSON)
Underlying Agent (Claude CLI)
```

The agent is always a separate process. `ail` communicates with it over stdin/stdout — for Claude CLI, this is `--output-format stream-json`. This boundary is architectural: the agent can be upgraded, swapped, or replaced without touching `ail`'s pipeline logic.

---

## Designed Features

The following are designed and specced in their current form. They haven't been built yet, so the designs haven't been tested against reality. Each links to the relevant spec section.

### Pipeline Inheritance (`FROM`) — [spec §7](spec/core/s07-pipeline-inheritance.md)

Intelligence is largely the capacity to recognize which existing knowledge structure applies and activate it — Schank and Abelson called these *scripts* in 1977. `ail`'s `FROM` inheritance is script instantiation in that precise sense: the payments team uses the org's base quality pipeline and adds a PCI check adjacent to the security audit. The base script is inherited unchanged. The instantiation supplies only what the domain requires.

> *** TODO: add that this was inspired by Dockerfiles, not Schank and Abelson 😄 

```yaml
FROM: /etc/ail/acme-engineering-base.yaml

pipeline:
  - run_before: security_audit
    id: pci_compliance_check
    provider: anthropic/claude-opus-4-5
    skill: ./skills/pci-checker/

  - disable: commit_checkpoint
```

Step IDs in an inheritable pipeline are a public API. Treat renames as breaking changes.

### Human-in-the-Loop Gates — [spec §13](spec/core/s13-hitl-gates.md)

Explicit pause points that wait for human approval before continuing. Also fires automatically when `on_result` detects a mismatch, or when the agent requests permission for a tool not covered by the step's policy. HITL is not an error state — it is the pipeline's comparison circuit surfacing a detected mismatch.

### Multi-Provider Routing — [spec §15](spec/core/s15-providers.md)

Cognitive flexibility means routing individual steps to the right model for the task: a fast cheap model for triage, a frontier model where it matters. The pipeline allocates attention deliberately; so does `ail`.

```yaml
providers:
  fast:     groq/llama-3.1-70b-versatile
  frontier: anthropic/claude-opus-4-5

pipeline:
  - id: syntax_triage
    provider: fast
    prompt: "Is the code above syntactically valid? Answer VALID or list issues."

  - id: architecture_review
    provider: frontier
    condition: if_code_changed
    prompt: ./prompts/architectural-review.md
```

### Skills — [spec §6](spec/core/s06-skills.md)

A *skill* is a directory with a `SKILL.md` file — natural language instructions read by the LLM, not the runtime. Skills are loaded via `append_system_prompt:` entries — you can stack multiple skills in explicit order alongside inline instructions. `ail` supports the [Agent Skills open standard](https://agentskills.io): skills authored for Claude, Gemini CLI, Copilot, or Cursor are directly usable without modification.

Note the deliberate departure from the standard: `ail` does not keep all skill metadata in context permanently. Surfacing a skill to the wrong step subjects it to the same attention degradation as everything else competing for the middle of the window. Selective context injection is the executive layer's job.

---

## Architecture

`ail` is built in Rust, structured as two crates:

```
ail-core/     — domain model, pipeline executor, runner adapters
ail/          — binary entry point, TUI, CLI argument parsing
```

The crate boundary is enforced: `ail-core` has no knowledge of the TUI or CLI. Both communicate through typed domain events. This separation means the same core powers the interactive TUI, headless mode, and the planned `ail serve` HTTP API without duplication.

For the full rationale — why Rust, the memory argument, runner adapter design, observability, and testing strategy — see [`ARCHITECTURE.md`](ARCHITECTURE.md).

---

## Roadmap

> TODO: this is an LLM hallucination - fix or remove.  Need a proper plan & roadmap. Real next goal: finish the spec, code a `v-alpha` reference implementation, build a swanky UI for humans, run SWEBench Pro through a trained `ail` session and get awesome results! (or prove that I'm actually just experiencing chatgpt psychosis and this is all a hilarious waste of time. Either way...

| Milestone | Focus |
|---|---|
| **v0.0.1** *(current)* | Parser, domain model, Claude CLI runner, step sequencing. Foundation only — no branching, conditions, or skills yet. |
| **v0.1** | `on_result` branching, conditions, `pause_for_human`, template variables, provider routing. First end-to-end working pipeline. |
| **v0.2** | `FROM` inheritance and hook operations, `skill:` field, `ail materialize`, `defaults:` block |
| **v0.3** | `ail serve` with OpenAPI spec and Swagger UI, headless mode, additional runners (Aider) |
| **Later** | Pipeline registry, safety guardrails, plugin extensibility, purpose-built web UI |

---

## Contributing

`ail` is in active early development. The spec (`SPEC.md`) describes intended behaviour as currently hypothesised. Implementation follows the spec, but the spec is expected to change as implementation reveals what works in practice. If you find something in the spec that seems wrong or unworkable, opening an issue is as valuable as writing code.

**Prerequisites:**
- Rust stable toolchain (`rustup`)
- `cargo-nextest` (`cargo install cargo-nextest`)
- Claude CLI installed and authenticated (for integration tests)

**Getting started:**

```bash
git clone https://github.com/AlexChesser/ail
cd ail
cargo build
cargo nextest run
```

**Viewing the full spec coverage picture:**

```bash
cargo nextest run --no-fail-fast --run-ignored all
```

**Debugging runner/provider issues:**

Enable structured NDJSON trace logging to see every event the claude CLI emits:

```bash
# --once mode: logs to stderr
RUST_LOG=ail_core::runner::claude=trace cargo run -- --once "hello" --pipeline demo/.include-review.yaml

# TUI mode: logs to ~/.ail/tui.log
RUST_LOG=ail_core::runner::claude=debug cargo run -- --pipeline demo/.include-review.yaml
tail -f ~/.ail/tui.log | jq .
```

The `debug` level logs event types, content block types, and `result` event fields. The `trace` level adds every raw NDJSON line. This is the primary tool for diagnosing why a provider's responses aren't appearing or why a pipeline step isn't continuing.

**Contributing a new feature:**
1. Check [`spec/README.md`](spec/README.md) to find the relevant section and its implementation status
2. Open an issue referencing the relevant spec section before beginning implementation work
3. Write the `spec_coverage.rs` test first — it defines the acceptance criteria
4. Implement until the test passes

The most valuable contribution right now is completing `on_result` branching ([spec §5.4](spec/core/s05-step-specification.md)). It is the next feature in the execution path and unlocks everything else.

---

## Documents

| Document | Contents |
|---|---|
| [`spec/README.md`](spec/README.md) | Navigation index for the AIL Pipeline Language Specification — per-section files with implementation status |
| [`spec/runner/`](spec/runner/) | The AIL Runner Contract — for CLI tool authors who want first-class `ail` compatibility |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Rust architecture, crate structure, domain model, testing strategy, and design principles |
| [`API.md`](API.md) | HTTP API surface design for the planned `ail serve` mode |
| [`docs/blog/the-yaml-of-the-mind.md`](docs/blog/the-yaml-of-the-mind.md) | The full cognitive science case for the executive function layer |
| [`CHANGELOG.md`](CHANGELOG.md) | Version history |

---

## The Trajectory: Self-Improving Loops

`ail` today is the contention scheduler — it executes the declared pipeline. The trajectory is toward the Supervisory Attentional System: `ail` selecting and composing pipelines appropriate to the task.

Norman and Shallice's SAS sits above contention scheduling — it intervenes when tasks are novel, ambiguous, or require overriding a habitual response. The pipeline run log accumulates structured evidence of how the agent fails and how those failures were resolved. The step that reads that log and proposes a YAML diff to the active pipeline is already expressible in the current spec:

```yaml
- id: pipeline_reflection
  prompt: |
    Review the attached pipeline run log.
    Identify the most common mismatch between intended and actual output.
    Propose a new step that would prevent it.
    Format your response as a YAML diff targeting the existing pipeline.
  on_result:
    always:
      action: pause_for_human
      message: "Pipeline improvement proposed. Approve to apply."
```

On approval, the diff is committed. The next invocation runs against an improved version of itself.

The empirical test is concrete and runnable: can a set of declared pipelines — linter, test runner, action acceptor, self-evaluation step — improve a model's own published SWE-bench score using that same model, with no changes to the weights? If the executive layer is doing real work, the benchmark moves. If it doesn't, the spec needs revision.

Either outcome is useful. The experiment can be designed. If you work in model evaluation, this is an invitation.

---

## License

`ail` uses different licenses for different artifacts, reflecting their different roles.

| Artifact | License | Rationale |
|---|---|---|
| `spec/` (all contents) | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) | Standards documents. Anyone can implement against them; derivative specs must remain open. |
| `ail-core/` | [MPL 2.0](https://www.mozilla.org/en-US/MPL/2.0/) | Usable in proprietary software; modifications to `ail-core` files themselves must be published. |
| `ail/` (the CLI binary) | [AGPL v3](https://www.gnu.org/licenses/agpl-3.0.html) | Running `ail serve` as a network service requires publishing modifications. |
| `demo/` | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) | Examples released into the public domain. Copy freely into any project. |

**Contributor License Agreement (CLA):** All contributors must sign the `ail` CLA before their pull requests can be merged. The CLA assigns copyright in your contributions to the project maintainer, preserving the ability to relicense any part of the project in the future.

---

*`ail` is built in public. The spec is the product. The implementation follows.*
