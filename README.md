# ail — Artificial Intelligence Loops

> **The executive function layer LLM agents structurally lack.**

[![Spec: CC BY-SA 4.0](https://img.shields.io/badge/spec-CC%20BY--SA%204.0-lightgrey.svg)](#license)
[![Core: MPL 2.0](https://img.shields.io/badge/core-MPL%202.0-blue.svg)](#license)
[![CLI: AGPL v3](https://img.shields.io/badge/cli-AGPL%20v3-red.svg)](#license)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status: Active Development](https://img.shields.io/badge/status-active%20development-yellow.svg)](#roadmap)

`ail` is an open-source pipeline runtime that wraps AI coding agents (starting with the Claude CLI) and automatically runs a deterministic chain of follow-up steps after every agent response — before control ever returns to the human.

---

## Quickstart

```bash
# Clone and build
git clone https://github.com/AlexChesser/ail
cd ail
cargo build --release

# Validate the demo pipeline
cargo run -- validate --pipeline demo/.ail.yaml

# Single-shot run (requires claude CLI installed and authenticated)
cd demo && ../target/release/ail "Write a function that adds two numbers" --pipeline .ail.yaml
```

For a taste of where this is headed, see the [Oh My AIL](demo/oh-my-ail/) demo — a multi-agent orchestration pipeline with intent classification, tiered routing, and role-specific tool permissions, all expressed as `.ail.yaml` files.

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

## What Works Today (v0.2)

`ail` is past the foundation stage. The core pipeline runtime is functional end-to-end.

**Pipeline execution:**
- `ail "my prompt" --pipeline .ail.yaml` — single-shot run with positional prompt (or `--once` long form)
- Steps execute in declaration order, each resuming the same Claude session (`--resume`)
- Template variables resolve across steps: `{{ step.<id>.response }}`, `{{ last_response }}`, `{{ env.VAR }}`, and [more](spec/core/s11-template-variables.md)
- `on_result` multi-branch evaluation with `contains:`, `exit_code:`, `always:` matchers and `continue`/`break`/`abort_pipeline`/`pause_for_human` actions
- `context: shell:` steps — run shell commands, capture stdout/stderr/exit_code, feed results into templates
- Sub-pipeline steps with isolation and depth guards
- Per-step `model:`, `tools:`, `resume:`, `system_prompt:`, `append_system_prompt:` overrides
- Controlled execution mode with `ExecutionControl` for programmatic consumers (NDJSON event stream)

**CLI commands:**
- `ail validate` — structured validation with typed errors
- `ail materialize` — resolved YAML output with `# origin` annotations
- `--show-work` — prints a summary of each completed step
- `--watch` — streams step responses as they complete
- `--output-format json` — NDJSON event stream for programmatic consumers

**Tooling:**
- [VS Code extension](vscode-ail-chat/) — chat interface, pipeline graph visualization, and language support for `.ail.yaml` files

**Architecture:**
- Two-crate Rust workspace (`ail-core` library + `ail` binary)
- `Runner` trait abstraction — `ClaudeCliRunner` is swappable; `StubRunner` for tests
- Append-only NDJSON turn log (`~/.ail/projects/<hash>/runs/<run_id>.jsonl`)
- RFC 9457-inspired structured errors with stable `error_type` constants
- `tracing`-based structured logging throughout

See the [CHANGELOG](CHANGELOG.md) for full version history.

---

## The Pipeline Language

`ail` pipelines are declared in a `.ail.yaml` file.

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

`ail` is built around a strict two-layer model:

| Layer | Format | Read by | Purpose |
|---|---|---|---|
| **Pipeline** | YAML | The `ail` runtime | Control flow — when, in what order, what to do with results |
| **Skill** | Markdown | The LLM | Instructions — how to think about and execute a task |

A pipeline orchestrates. A skill instructs. They are complementary, not interchangeable. Adding more skill instructions to a context saturation problem just makes a larger context. `ail` operates at the layer that decides what goes into the context at all.

Full language documentation is in [`spec/README.md`](spec/README.md).

---

## Pipeline File Discovery

1. Explicit `--pipeline <path>` flag
2. `.ail.yaml` in CWD
3. `.ail/default.yaml` in CWD
4. `~/.config/ail/default.yaml`

If nothing found, `ail` runs in passthrough mode — transparent, zero-config, pipeline = invocation only.

---

## Scope Discipline

The compass for every implementation decision is: *does this serve the frontal lobe?*

A feature belongs in `ail` if it:
- **Addresses one of the four failure modes** — perseveration, goal substitution, source monitoring failure, or anosognosia; or
- **Strengthens one of Diamond's three executive function components** — inhibitory control, working memory updating, or cognitive flexibility; or
- **Extends `ail`'s capacity to select, compose, or improve its own pipelines** — the supervisory layer that decides *which* script to run, not just *that* it runs.

Features that serve general task execution without mapping to any of these categories belong to the agent layer beneath `ail`, not to the control plane above it.

---

## Designed But Not Yet Built

### Pipeline Inheritance (`FROM`) — [spec §7](spec/core/s07-pipeline-inheritance.md)

```yaml
FROM: /etc/ail/acme-engineering-base.yaml

pipeline:
  - run_before: security_audit
    id: pci_compliance_check
    provider: anthropic/claude-opus-4-5
    skill: ./skills/pci-checker/

  - disable: commit_checkpoint
```

Inherit a base pipeline and surgically modify it. Step IDs in an inheritable pipeline are a public API — treat renames as breaking changes.

### Multi-Provider Routing — [spec §15](spec/core/s15-providers.md)

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

Route individual steps to the right model for the task: a fast cheap model for triage, a frontier model where it matters.

### Skills — [spec §6](spec/core/s06-skills.md)

A *skill* is a directory with a `SKILL.md` file — natural language instructions read by the LLM, not the runtime. `ail` supports the [Agent Skills open standard](https://agentskills.io): skills authored for Claude, Gemini CLI, Copilot, or Cursor are directly usable without modification.

---

## Architecture

`ail` is built in Rust, structured as two crates:

```
ail-core/     — domain model, pipeline executor, runner adapters, template engine
ail/          — binary entry point, CLI argument parsing, output formatting
```

The crate boundary is enforced: `ail-core` has no knowledge of the CLI. Both communicate through typed domain events. This separation means the same core powers the CLI, headless mode, and the planned `ail serve` HTTP API without duplication.

For the full rationale — why Rust, the memory argument, runner adapter design, observability, and testing strategy — see [`ARCHITECTURE.md`](ARCHITECTURE.md).

---

## Roadmap

| Milestone | Focus |
|---|---|
| **v0.0.1** | Parser, domain model, Claude CLI runner, step sequencing |
| **v0.1** | `on_result` branching, `context: shell:` steps, template variables, headless mode |
| **v0.2** *(current)* | Transparent passthrough, lean output, `--show-work`, `--watch`, TUI removal, sub-pipelines, controlled execution |
| **v0.3** | `skill:` step execution, `FROM` inheritance, additional conditions |
| **v0.4** | `ail serve` HTTP API, additional runners, `ail log` / `ail logs` commands |
| **v0.5** | Interactive REPL, pipeline registry, self-improving loops |

---

## Contributing

`ail` is in active development. The spec ([`spec/README.md`](spec/README.md)) describes intended behaviour — it is the primary published artifact. Implementation follows the spec, but the spec changes as reality pushes back.

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

**Debugging runner issues:**

```bash
RUST_LOG=ail_core::runner::claude=trace cargo run -- --once "hello" --pipeline demo/.ail.yaml
```

The `debug` level logs event types and content block types. The `trace` level adds every raw NDJSON line.

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
## Star History

[![Star History Chart](https://api.star-history.com/image?repos=alexchesser/ail&type=date&legend=top-left)](https://www.star-history.com/?repos=alexchesser%2Fail&type=date&legend=top-left)


*`ail` is built in public. The spec is the product. The implementation follows.*
