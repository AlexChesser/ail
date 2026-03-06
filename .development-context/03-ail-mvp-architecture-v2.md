# `ail` MVP Architecture — v0.0.1 Proof of Concept

> Cross-referenced against: `00-initial-prompt.md`, `01-initial-response.md`, and `SPEC.md §20`

---

## 1. What This Document Is

The initial architecture response (`01-initial-response.md`) was written against the full technical ambitions of `00-initial-prompt.md` — a maximalist vision involving PTY management, PID-controlled meta-learning, Redis/PostgreSQL state, distributed tracing, Git-backed YAML mutation, and multi-model parallel critique loops.

The `SPEC.md` has since clarified what the system actually *is*: a YAML-orchestrated pipeline runtime whose core invariant is dead simple — *after an agent finishes, run the configured steps, in order, before returning control to the human.*

`SPEC.md §20` already defines v0.0.1 scope explicitly. This document translates that into a concrete Rust implementation plan, and explicitly annotates what from the initial architecture is deferred, dropped, or preserved.

---

## 2. The MVP in One Sentence

> Read a `.ail.yaml` from the current directory, parse the `pipeline` array, and for each step execute `claude --print "<prompt>"` in sequence, streaming output to a Ratatui TUI. Exit when the last step exits 0.

That is the entire proof of concept. Everything else is deferred.

---

## 3. What the MVP Must Do (from SPEC.md §20)

| SPEC Requirement | MVP Implementation |
|---|---|
| Single `.ail.yaml` in current directory | `std::fs::read_to_string(".ail.yaml")` — no discovery chain yet |
| `pipeline:` array, top-to-bottom | `Vec<Step>` iterated sequentially |
| `prompt:` field — inline string only | Passed directly to `claude --print` |
| `id:` field | Parsed and stored; used for display in TUI only |
| `provider:` field | Parsed but **ignored in MVP** — hardcoded to `claude` CLI |
| `condition: always` / `condition: never` | Trivial: skip step if `never`, run if `always` or absent |
| `on_result: contains` + `continue` / `abort_pipeline` | String match on captured stdout; `pause_for_human` deferred |
| `{{ last_response }}` template variable | String replacement before each `claude --print` call |
| Passthrough mode (no `.ail.yaml` found) | Print message and exit 0 |
| Basic TUI — streaming stdout passthrough | Ratatui with a scrolling output panel |
| Completion detection via process exit 0 | `std::process::Command` wait, check exit code |

---

## 4. Deferral Map — Initial Architecture vs. MVP

Everything in the initial response is deferred unless explicitly listed as included below.

### 4.1 Deferred Entirely (do not implement)

| Initial Architecture Component | Why Deferred |
|---|---|
| **PTY / `portable-pty`** | `claude --print` is non-interactive; process exit signals completion. PTY is only needed for interactive session wrapping, which is not in scope. |
| **`InteractivePrompt` bubble-up** | No interactive prompts in `--print` mode. |
| **Janitor / Context Distillation** | A pipeline step concern, not a runtime concern. When needed, it will be a `skill:` step in YAML. Not built into the kernel. |
| **`DistilledContext` type** | Follows from Janitor deferral. |
| **Recursive Meta-Learning / PID controller** | §21 Planned Extensions. Not in v0.0.1. |
| **Parallel model prompting** | §21 Planned Extensions. |
| **YAML mutation / `git2`** | §21 Planned Extensions. |
| **Redis** | No external state store needed. MVP carries all state in-process for the duration of one pipeline run. |
| **PostgreSQL / audit log** | No persistence needed in MVP. |
| **LiteLLM / Bifrost proxy** | `provider:` field parsed but ignored. Hardcode `claude` CLI. |
| **Docker Compose topology** | Single binary, runs locally. |
| **Circuit breakers (budget, confidence, risk)** | No LLM API calls directly — no token/cost visibility from CLI invocation. |
| **HITL gates (`pause_for_human`)** | Explicitly out of scope per SPEC §20. |
| **`LoopState` state machine** | Overkill for a linear step-by-step executor. Replace with simple iteration. |
| **`TuiDirective` / `KernelCommand` channel protocol** | The full async message-passing architecture is unnecessary. One channel from subprocess stdout to TUI is sufficient. |
| **`X-AIL-*` HTTP headers / proxy instrumentation** | No HTTP layer in MVP. |
| **15-Factor compliance (Factors VI, XI, XV)** | Valid long-term targets. Not enforced in MVP — state lives in-process. |
| **DDD ubiquitous language structs** (`AggregateRoot`, `RefinementLoop`, etc.) | Don't add these until the domain complexity warrants them. |
| **Hexagonal / Ports & Adapters** | Valid long-term architecture. Not required for a 3-module MVP. |
| **`materialize-chain` CLI command** | SPEC §20 lists it as in-scope but it's a no-op for a single-file pipeline with no inheritance. Implement as a stub that prints the resolved YAML and exits. |

### 4.2 Preserved from Initial Architecture (scaled down)

| Initial Architecture Component | MVP Form |
|---|---|
| **Ratatui TUI** | Yes — but minimal. A scrolling output pane and a status bar. No modal overlays, no HITL panels. |
| **Tokio async runtime** | Yes — used for the subprocess stdout stream → TUI render loop. Single producer, single consumer. |
| **`ModelProvider` trait** | Define the trait; ship one concrete implementation: `ClaudeCliProvider`. Swap-in for other providers later without changing the executor. |
| **`Config::from_env()`** | Minimal: `AIL_CLAUDE_BIN` env var overrides the `claude` binary path. Nothing else. |
| **Structured error types** | Yes — `AilError` enum with `YamlParseError`, `StepExecutionError`, `TemplateError`. No panics. |

---

## 5. MVP Module Structure

Three modules. That's it.

```
ail/
├── Cargo.toml
└── src/
    ├── main.rs          # Entry point: parse args, load config, run pipeline, render TUI
    ├── pipeline.rs      # YAML parsing: PipelineConfig, Step, Condition, OnResult
    ├── executor.rs      # Step execution: spawn claude CLI, stream stdout, check exit
    └── tui.rs           # Ratatui setup: output scroll pane, status bar
```

No `adapters/`, no `ports/`, no `domain/` directories. Add those when the domain actually needs them.

---

## 6. Core Data Structures (MVP Only)

```rust
// pipeline.rs

#[derive(Debug, serde::Deserialize)]
pub struct PipelineConfig {
    pub version: String,
    pub pipeline: Vec<Step>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Step {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub condition: Condition,
    pub on_result: Option<OnResult>,
    // provider: parsed, ignored in MVP
    pub provider: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    #[default]
    Always,
    Never,
}

#[derive(Debug, serde::Deserialize)]
pub struct OnResult {
    pub contains: String,
    pub if_true: OnResultAction,
    pub if_false: OnResultAction,
}

#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OnResultAction {
    Continue,
    AbortPipeline,
    PauseForHuman, // parsed, not yet implemented — log warning and treat as Continue
}
```

```rust
// executor.rs

pub trait ModelProvider: Send + Sync {
    /// Execute a prompt and return the full response as a String.
    async fn run(&self, prompt: &str) -> Result<String, AilError>;
}

pub struct ClaudeCliProvider {
    pub binary: PathBuf, // from AIL_CLAUDE_BIN or "claude"
}

impl ModelProvider for ClaudeCliProvider {
    async fn run(&self, prompt: &str) -> Result<String, AilError> {
        // tokio::process::Command::new(&self.binary)
        //   .args(["--print", prompt])
        //   .output()
        //   .await
        // Check exit code. Collect stdout as String.
    }
}
```

---

## 7. Template Variable Resolution (MVP)

Only `{{ last_response }}` is implemented. The substitution is a simple `str::replace` before each step's prompt is sent to the provider.

`{{ step.invocation.response }}` is listed in SPEC §20 as in-scope. For MVP, treat it as an alias for `last_response` — the invocation response is the content of the user's session before `ail` ran. In `--print` mode this is unavailable, so substitute with empty string and log a warning. Revisit when session integration is understood (SPEC §22, Open Question: Context Accumulation).

---

## 8. TUI (Ratatui) — MVP Scope

One layout. Two widgets.

```
┌─────────────────────────────────────────┐
│ ail — pipeline: <name>    step 2/3      │  ← status bar
├─────────────────────────────────────────┤
│                                         │
│  [stdout from current step scrolls      │
│   here in real time]                    │
│                                         │
└─────────────────────────────────────────┘
```

- No modal overlays.
- No HITL panels.
- No metrics / budget displays.
- Scroll with arrow keys; `q` to abort pipeline.

The async boundary is simple: `tokio::process` streams stdout bytes via a channel to the Ratatui render loop. No `PtyEvent` enum needed — just `Vec<u8>` chunks.

---

## 9. What the MVP Deliberately Does Not Validate

The following are real risks from the initial architecture but are out of scope for a proof of concept:

| Risk (from initial response §4.4) | MVP Stance |
|---|---|
| Feedback loop hallucination / mutation drift | Not applicable — no mutation engine. |
| PTY prompt detection false negative | Not applicable — `--print` mode only. |
| Redis state corruption | Not applicable — no Redis. |

The one real MVP risk: **`claude --print` exit code semantics are unresolved** (SPEC §22, Open Question: Completion Detection). The first implementation spike must validate that `claude --print "<prompt>"` exits 0 on success and non-zero on error across target environments. This is a blocking unknown before any other code is written.

---

## 10. The v0.0.1 Success Condition

Given this `.ail.yaml` in the current directory:

```yaml
version: "0.0.1"

pipeline:
  - id: dont_be_stupid
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

Running `ail` should:

1. Parse the file.
2. Execute `claude --print "Review the above output..."`.
3. Stream the response to the TUI.
4. Exit 0.

That's the demo. It validates the core invariant. Everything in the initial architecture document remains valid as a *long-term target* — none of it is wrong, it is simply not yet needed.

---

## 11. Implementation Order

1. **Spike:** Validate `claude --print` exit code behaviour. (Blocking — do this first.)
2. **`pipeline.rs`:** YAML parsing with `serde_yaml`. Hardcode `version: "0.0.1"` acceptance.
3. **`executor.rs`:** `ClaudeCliProvider` with `tokio::process::Command`. No TUI yet — print to stdout.
4. **`tui.rs`:** Add Ratatui. Wire stdout stream to scroll pane.
5. **Template substitution:** `{{ last_response }}` replacement.
6. **`on_result`:** `contains` + `continue` / `abort_pipeline`.
7. **`condition: never`:** Skip logic.
8. **Passthrough mode:** No `.ail.yaml` → exit 0 with message.
9. **`materialize-chain` stub:** Print parsed pipeline as YAML, exit.
