# ail-core — Library Crate

All business logic lives here. No CLI, no user interaction.
Consumed by `ail` (the binary) and future language-server / SDK targets.

## Module Responsibilities

| Module | Responsibility |
|---|---|
| `config/discovery.rs` | Walk the four-step file resolution order (SPEC §3.1) |
| `config/dto.rs` | Serde-deserialised raw structs — derives `Deserialize` |
| `config/domain.rs` | Validated domain types — no `Deserialize` derives |
| `config/validation.rs` | `dto → domain` conversion with typed `AilError` on failure |
| `config/mod.rs` | `load(path)` public entry point |
| `error.rs` | `AilError`, `ErrorContext`, `error_types` string constants |
| `executor.rs` | `execute(&mut Session, &dyn Runner)` — SPEC §4.2 core invariant |
| `materialize.rs` | `materialize(&Pipeline) → String` — annotated YAML round-trip |
| `runner/mod.rs` | `Runner` trait, `RunResult`, `InvokeOptions` |
| `runner/claude.rs` | `ClaudeCliRunner` — shells out to the `claude` CLI |
| `runner/stub.rs` | `StubRunner` — deterministic test double |
| `session/state.rs` | `Session` — `run_id`, `pipeline`, `invocation_prompt`, `turn_log` |
| `session/turn_log.rs` | `TurnLog` — append-only NDJSON writer + in-memory entries |
| `template.rs` | `resolve(template, &Session) → Result<String, AilError>` |

## Key Types

```rust
// Pipeline and its steps
pub struct Pipeline { pub steps: Vec<Step>, pub source: Option<PathBuf> }
pub struct Step    { pub id: StepId, pub body: StepBody, pub tools: Option<ToolPolicy> }
pub enum StepBody  { Prompt(String), Skill(PathBuf), SubPipeline(PathBuf), Action(ActionKind) }
pub enum ActionKind { PauseForHuman }

// Runner contract
pub trait Runner { fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError>; }
pub struct RunResult { pub response: String, pub cost_usd: Option<f64>, pub session_id: Option<String> }
pub struct InvokeOptions { pub resume_session_id: Option<String>, pub allowed_tools: Vec<String>, pub denied_tools: Vec<String> }

// Session
pub struct Session { pub run_id: String, pub pipeline: Pipeline, pub invocation_prompt: String, pub turn_log: TurnLog }

// Error
pub struct AilError { pub error_type: &'static str, pub title: &'static str, pub detail: String, pub context: Option<ErrorContext> }
```

## Error Types (`error::error_types`)

| Constant | Value |
|---|---|
| `CONFIG_INVALID_YAML` | `ail:config/invalid-yaml` |
| `CONFIG_FILE_NOT_FOUND` | `ail:config/file-not-found` |
| `CONFIG_VALIDATION_FAILED` | `ail:config/validation-failed` |
| `TEMPLATE_UNRESOLVED` | `ail:template/unresolved-variable` |
| `RUNNER_INVOCATION_FAILED` | `ail:runner/invocation-failed` |
| `PIPELINE_ABORTED` | `ail:pipeline/aborted` |

## Invariants (do not break)

1. **SPEC §4.2 core invariant**: once `execute()` begins, all steps run in order. Early exit only via explicit declared outcomes — never silent failures.
2. Template resolution failure **aborts the step before the runner is called** — no TurnEntry recorded for the failed step.
3. Intent is recorded (`record_step_started`) before the runner is called — crash evidence.
4. `ClaudeCliRunner` must call `.env_remove("CLAUDECODE")` on `Command` to avoid nested-session guard.
5. `--output-format stream-json` **requires** `--verbose` when combined with `-p`.

## Rules

- No `unwrap()`/`expect()` — use `?` and `AilError`
- No `println!`/`eprintln!` — use `tracing::{info, warn, error}`
- `dto.rs` only: `#[derive(Deserialize)]`
- `domain.rs` only: clean domain types, no serde
- Modules returning `Result<_, AilError>` need `#[allow(clippy::result_large_err)]`

## Testing

Tests live in `ail-core/tests/spec/` with one file per SPEC section:

```
s03 — file format / YAML parsing
s04 — execution model
s05 — step specification
s08 — runner adapter
s09 — tool permissions
s11 — template variables
s18 — materialize
s21 — MVP scope
```

`#[ignore]` tests require the `claude` binary and a live session — run with
`cargo nextest run --include-ignored` outside a Claude Code session.

Fixtures: `ail-core/tests/fixtures/` — minimal, solo_developer, invalid_* YAML variants.
