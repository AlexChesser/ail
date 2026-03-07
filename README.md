# `ail` — Alexander's Impressive Loops (AI Loops) 

> **The control plane for how AI coding agents behave after the human stops typing.**

[![Spec: CC BY-SA 4.0](https://img.shields.io/badge/spec-CC%20BY--SA%204.0-lightgrey.svg)](#license)
[![Core: MPL 2.0](https://img.shields.io/badge/core-MPL%202.0-blue.svg)](#license)
[![CLI: AGPL v3](https://img.shields.io/badge/cli-AGPL%20v3-red.svg)](#license)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Active Development](https://img.shields.io/badge/status-active%20development-yellow.svg)](#roadmap)

`ail` is an open-source pipeline runtime that wraps AI coding agents like the Claude CLI and automatically runs a deterministic chain of follow-up prompts after every agent response — before control ever returns to the human.

Write a `.ail.yaml` file. Every time your AI coding agent finishes, your quality gates run. Every time. Without you having to remember to ask.

> ⚠️ **This project is in early development.** The parser and domain model are working, but the executor is not yet complete. The pipeline language is a working hypothesis — it feels solid, but real-world implementation will test that. The examples below show what `ail` is being built toward — not what it does today. See [Current Status](#current-status) for what is and isn't implemented.

---

## The Problem

Current agentic coding tools treat the human prompt as a single, transactional event. If you want a DRY refactor, a security audit, or a test suite written after the agent produces code — you have to ask for it manually, every single time.

This creates two problems:

1. **Inconsistency.** You remember to ask for the security audit on Tuesday. You forget on Thursday. The codebase diverges.
2. **Prompt fatigue.** Typing the same follow-up chain repeatedly is mechanical, error-prone, and slow.

`ail` aims to solve both by making your quality pipeline a declared artifact — version-controlled, shareable, and automatically enforced.

---

## The Core Guarantee

> For every completion event produced by an underlying agent, `ail` will begin executing the pipeline defined in the active `.ail.yaml` file before control returns to the human.

Steps execute in order. Individual steps may be skipped by declared conditions or disabled explicitly. Execution may terminate early via `break`, `abort_pipeline`, or an unhandled error. All of these are explicit, declared outcomes — not silent failures.

*This is the guarantee the project is being built toward. Whether the design as specified actually delivers it cleanly is what implementation will tell us.*

---

## The Pipeline Language

`ail` pipelines are declared in a `.ail.yaml` file. The examples below show the intended syntax as currently designed. The spec is a working hypothesis — details may change as implementation reveals what works and what doesn’t. See [Current Status](#current-status) for what is running today.

### The Simplest Possible Pipeline

```yaml
# .ail.yaml
version: "0.1"

pipeline:
  - id: invocation
    prompt: "{{ session.invocation_prompt }}"
```

`invocation` is always step zero — it represents the human's prompt and the agent's response to it. A pipeline with only `invocation` is a valid passthrough: the agent runs normally and nothing extra fires. Add steps below it and they run automatically every time, before control returns to you.

### One Step Further

```yaml
version: "0.1"

pipeline:
  - id: invocation
    prompt: "{{ session.invocation_prompt }}"

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
  - id: invocation
    prompt: "{{ session.invocation_prompt }}"

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

`ail` is built around a strict two-layer model:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable. Full language documentation is in [`SPEC.md`](SPEC.md).

---

## Current Status

The project is being built spec-first. The spec represents a hypothesis — a best guess at what a high-quality agentic developer workflow looks like, written before most of it has been built. Implementation is the experiment that will validate or refute that hypothesis. Things that sound good on paper may turn out to be awkward in practice. The spec will change as reality pushes back.

The goal is that by v1.0, everything in the spec has been earned through working implementation — not just designed. Until then, treat the spec as a proposal and an invitation for feedback, not a stable contract.

Implementation progress is tracked via a dedicated spec coverage test suite.

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

Everything in the [Planned Extensions](SPEC.md#21-planned-extensions) section of the spec is also unimplemented.

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

The following are designed and specced in their current form. They haven’t been built yet, so the designs haven’t been tested against reality. Each links to the relevant spec section — feedback on the design is welcome.

### Pipeline Inheritance (`FROM`) — [SPEC §7](SPEC.md)

Teams share base pipelines. Projects extend them. Individuals customise further. Hook operations (`run_before`, `run_after`, `override`, `disable`) let inheriting pipelines modify inherited steps without forking.

```yaml
FROM: /etc/ail/acme-engineering-base.yaml

pipeline:
  - run_before: security_audit
    id: pci_compliance_check
    provider: anthropic/claude-opus-4-5
    skill: ./skills/pci-checker/

  - disable: commit_checkpoint
```

### Human-in-the-Loop Gates — [SPEC §13](SPEC.md)

Explicit pause points that wait for human approval before continuing. Also fires automatically when `on_result` detects a problem, or when the agent requests permission to use a tool not covered by the step's tool policy.

### Multi-Provider Routing — [SPEC §15](SPEC.md)

Route individual steps to different models. Use a fast cheap model for triage steps, a frontier model for the steps that matter.

### Skills — [SPEC §6](SPEC.md)

A *skill* is a directory with a `SKILL.md` file — natural language instructions read by the LLM, not the runtime. `ail` will support the [Agent Skills open standard](https://agentskills.io), making skills authored for Claude, Gemini CLI, Copilot, or Cursor directly usable without modification.

### `ail serve` — HTTP API Mode — [API.md](API.md)

A planned operating mode that exposes the full pipeline executor as an HTTP API with an auto-generated OpenAPI 3.1 spec, SSE streaming, and a built-in Swagger UI. Enables auto-generated native clients in any language and agent-driven pipeline execution without a human present.

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

**Contributing a new feature:**
1. Check `SPEC.md` to understand the intended behaviour
2. Open an issue referencing the relevant spec section before beginning implementation work
3. Write the `spec_coverage.rs` test first — it defines the acceptance criteria
4. Implement until the test passes

The most valuable contribution right now is completing `on_result` branching ([SPEC §5.3](SPEC.md)). It is the next feature in the execution path and unlocks everything else.

---

## Documents

| Document | Contents |
|---|---|
| [`SPEC.md`](SPEC.md) | The AIL Pipeline Language Specification — the current working hypothesis for `.ail.yaml` syntax and semantics. Expected to evolve as implementation proceeds. |
| [`RUNNER-SPEC.md`](RUNNER-SPEC.md) | The AIL Runner Contract — for CLI tool authors who want first-class `ail` compatibility |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Rust architecture, crate structure, domain model, testing strategy, and design principles |
| [`API.md`](API.md) | HTTP API surface design for the planned `ail serve` mode |
| [`CHANGELOG.md`](CHANGELOG.md) | Version history |

---

## License

`ail` uses different licenses for different artifacts, reflecting their different roles and the principle that it is easier to relax a license later than to tighten it.

| Artifact | License | Rationale |
|---|---|---|
| `spec/` (all contents) | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) | The specs are standards documents. CC BY-SA allows anyone to implement against them freely, but derivative specs must remain open under the same terms. Encourages ecosystem formation without allowing the standard to be forked and closed. |
| `ail-core/` | [MPL 2.0](https://www.mozilla.org/en-US/MPL/2.0/) | The core library can be used in proprietary software, but modifications to `ail-core` files themselves must be published under MPL 2.0. File-level copyleft: your code stays yours, improvements to the engine stay open. |
| `ail/` (the CLI binary) | [AGPL v3](https://www.gnu.org/licenses/agpl-3.0.html) | Anyone running `ail` as a network service — the `ail serve` use case — must publish their modifications. Prevents proprietary managed `ail serve` offerings that don't give back. |
| `demo/` | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) | Examples and demo pipelines are released into the public domain. No attribution required, no conditions. Copy freely into any project, proprietary or otherwise. |

**Contributor License Agreement (CLA):** All contributors must sign the `ail` CLA before their pull requests can be merged. The CLA assigns copyright in your contributions to the project maintainer. This preserves the ability to relicense any part of the project in the future without needing to track down and re-obtain permission from every contributor. The CLA agreement will be linked here once the tooling is in place.

---

*`ail` is built in public. The spec is the product. The implementation follows.*