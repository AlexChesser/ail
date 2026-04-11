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
| `runner/subprocess.rs` | `SubprocessSession` — generic CLI subprocess lifecycle (spawn, stderr drain, cancel watchdog, reap); shared by all CLI-based runners |
| `runner/claude/mod.rs` | `ClaudeCliRunner` — orchestrates subprocess + decoder + permission listener |
| `runner/claude/decoder.rs` | `ClaudeNdjsonDecoder` — stateful NDJSON stream decoder, no process coupling; unit-testable with raw byte strings |
| `runner/claude/permission.rs` | `ClaudePermissionListener` — RAII guard for the tool-permission socket (hook settings file, accept loop, `__close__` sentinel, cleanup on drop) |
| `runner/factory.rs` | `RunnerFactory` — builds runners by name; honours `AIL_DEFAULT_RUNNER` env |
| `runner/http.rs` | `HttpRunner` — direct OpenAI-compatible HTTP runner (Ollama, direct API); full system-prompt control, think flag, in-memory session continuity |
| `runner/stub.rs` | `StubRunner`, `CountingStubRunner`, `EchoStubRunner`, `RecordingStubRunner` — deterministic test doubles |
| `session/log_provider.rs` | `LogProvider` trait + `JsonlProvider` (NDJSON) + `NullProvider` (tests) |
| `session/state.rs` | `Session` — `run_id`, `pipeline`, `invocation_prompt`, `turn_log` |
| `session/turn_log.rs` | `TurnLog` — in-memory entry store + delegates persistence to `LogProvider` |
| `template.rs` | `resolve(template, &Session) → Result<String, AilError>` |

## Key Types

```rust
// Pipeline and its steps
pub struct Pipeline { pub steps: Vec<Step>, pub source: Option<PathBuf> }
pub struct Step    { pub id: StepId, pub body: StepBody, pub tools: Option<ToolPolicy>, pub on_result: Option<Vec<ResultBranch>>, pub model: Option<String>, pub runner: Option<String>, pub condition: Option<Condition> }
pub enum Condition { Always, Never }  // SPEC §12 — None means Always; Never skips the step
pub struct Pipeline { pub steps: Vec<Step>, pub source: Option<PathBuf>, pub defaults: ProviderConfig, pub default_tools: Option<ToolPolicy> }
// default_tools: pipeline-wide fallback; per-step tools override entirely (SPEC §3.2)
pub struct Step    { pub id: StepId, pub body: StepBody, pub tools: Option<ToolPolicy>, pub on_result: Option<Vec<ResultBranch>>, pub model: Option<String>, pub runner: Option<String> }
pub enum StepBody  { Prompt(String), Skill(PathBuf), SubPipeline { path: String, prompt: Option<String> }, Action(ActionKind), Context(ContextSource) }
// SubPipeline.path may contain {{ variable }} syntax — resolved at execution time (SPEC §11)
// SubPipeline.prompt: when Some, overrides child session's invocation_prompt instead of using parent's last_response (SPEC §9.3)
pub enum ContextSource { Shell(String) }
pub enum ActionKind { PauseForHuman }
pub struct ResultBranch { pub matcher: ResultMatcher, pub action: ResultAction }
pub enum ResultMatcher { Contains(String), ExitCode(ExitCodeMatch), Always }
pub enum ExitCodeMatch { Exact(i32), Any }
pub enum ResultAction { Continue, Break, AbortPipeline, PauseForHuman, Pipeline { path: String, prompt: Option<String> } }
// Pipeline.path may contain {{ variable }} syntax — resolved at execution time (SPEC §11)
// Pipeline.prompt: when Some, overrides child session's invocation_prompt instead of using parent's last_response (SPEC §9.3)
// const MAX_SUB_PIPELINE_DEPTH: usize = 16 — enforced by execute_inner depth counter

// Provider/model config (SPEC §15) — resolved chain: defaults → per-step → cli_provider
pub struct ProviderConfig { pub model: Option<String>, pub base_url: Option<String>, pub auth_token: Option<String> }
// merge(self, other): other wins on conflict; absent fields fall through from self
// Note: input_cost_per_1k / output_cost_per_1k removed (dead code — cost is in RunResult.cost_usd)

// Runner contract
pub trait Runner { fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError>; }
// RunnerFactory (runner/factory.rs) — resolves runners by name
// Selection hierarchy: per-step runner: field → AIL_DEFAULT_RUNNER env → "claude"
pub struct RunnerFactory;
// RunnerFactory::build(name, headless) -> Result<Box<dyn Runner + Send>, AilError>
// RunnerFactory::build_default(headless) -> Result<Box<dyn Runner + Send>, AilError>
pub struct ToolEvent { pub event_type: String, pub tool_name: String, pub tool_id: String, pub content_json: String, pub seq: i64 }
// event_type: "tool_call" or "tool_result"; tool_name is empty for tool_result (not in wire format)
pub struct RunResult { pub response: String, pub cost_usd: Option<f64>, pub session_id: Option<String>, pub input_tokens: u64, pub output_tokens: u64, pub thinking: Option<String>, pub model: Option<String>, pub tool_events: Vec<ToolEvent> }
pub type PermissionResponder = Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync>;
pub struct PermissionRequest { pub display_name: String, pub display_detail: String, pub tool_input: Option<serde_json::Value> }
// display_detail is pre-formatted by the runner from its native tool input format.
// tool_input: raw JSON tool input from the runner; used by AskUserQuestion intercept. ClaudeCliRunner populates; others may leave None.
pub enum ToolPermissionPolicy { RunnerDefault, NoTools, Allowlist(Vec<String>), Denylist(Vec<String>), Mixed { allow: Vec<String>, deny: Vec<String> } }
// NoTools → --tools "" on ClaudeCliRunner; disables all tool calls. ToolPolicy.disabled=true maps to this.
pub struct InvokeOptions { pub resume_session_id: Option<String>, pub tool_policy: ToolPermissionPolicy, pub model: Option<String>, pub extensions: Option<Box<dyn Any + Send>>, pub permission_responder: Option<PermissionResponder> }
// extensions: runners downcast to their own type (e.g. ClaudeInvokeExtensions { base_url, auth_token, permission_socket }).
// Executor packs ClaudeInvokeExtensions pragmatically; to be injected via runner config in task 04.
// permission_responder: when set, the runner intercepts tool permission requests and calls this callback.
// ClaudeCliRunner encapsulates the Unix socket lifecycle internally; the TUI never manages socket paths.

// Claude CLI runner config (runner/claude.rs) — builder for ClaudeCliRunner
pub struct ClaudeCliRunnerConfig { pub claude_bin: String, pub headless: bool }
// impl Default: claude_bin="claude", headless=false
// Builder: .headless(bool), .claude_bin(str), .build() -> ClaudeCliRunner
// ClaudeCliRunner::from_config(config) is the preferred constructor

// Claude CLI runner extensions (runner/claude.rs)
pub struct ClaudeInvokeExtensions { pub base_url: Option<String>, pub auth_token: Option<String>, pub permission_socket: Option<PathBuf> }

// HTTP runner config (runner/http.rs) — calls any OpenAI-compatible /v1/chat/completions endpoint
pub struct HttpRunnerConfig { pub base_url: String, pub auth_token: Option<String>, pub default_model: Option<String>, pub think: Option<bool> }
// Default base_url: "http://localhost:11434/v1" (local Ollama)
// think: Some(false) disables extended thinking for qwen3 and similar models
// HttpRunner::ollama(model) convenience ctor: base_url=localhost:11434, think=Some(false)
// Factory names: "http" or "ollama" — reads AIL_HTTP_BASE_URL, AIL_HTTP_TOKEN, AIL_HTTP_MODEL, AIL_HTTP_THINK
// Session continuity: maintained in-memory; session_id is a UUID returned by invoke() and passed as resume_session_id by the executor

// Executor outcome
pub enum ExecuteOutcome { Completed, Break { step_id: String } }

// Session
pub struct Session { pub run_id: String, pub pipeline: Pipeline, pub invocation_prompt: String, pub turn_log: TurnLog, pub cli_provider: ProviderConfig, pub cwd: String, pub runner_name: String, pub headless: bool }
// cwd: captured at Session::new() time via std::env::current_dir(); used by {{ session.cwd }} template variable.
// TurnEntry carries prompt-step fields (response, runner_session_id, thinking, tool_events) and context-step fields (stdout, stderr, exit_code)
// tool_events: Vec<ToolEvent> — populated from RunResult.tool_events for prompt steps; empty for context/action/sub-pipeline steps

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
| `RUNNER_NOT_FOUND` | `ail:runner/not-found` |
| `PIPELINE_ABORTED` | `ail:pipeline/aborted` |
| `STORAGE_QUERY_FAILED` | `ail:storage/query-failed` |
| `RUN_NOT_FOUND` | `ail:storage/run-not-found` |
| `STORAGE_DELETE_FAILED` | `ail:storage/delete-failed` |

## Invariants (do not break)

1. **SPEC §4.2 core invariant**: once `execute()` begins, all steps run in order. Early exit only via explicit declared outcomes — never silent failures.
2. Template resolution failure **aborts the step before the runner is called** — no TurnEntry recorded for the failed step.
3. Template resolution applies to **both `pipeline:` paths and `on_result: pipeline:` action values** (SPEC §11) — resolved at execution time, not parse time.
4. Intent is recorded (`record_step_started`) before the runner is called — crash evidence.
4. `ClaudeCliRunner` must call `.env_remove("CLAUDECODE")` on `Command` to avoid nested-session guard.
5. `--output-format stream-json` **requires** `--verbose` when combined with `-p`.
6. **Context steps bypass the Runner** — `context: shell:` spawns `/bin/sh -c` directly; `Runner::invoke` is never called.
7. **Non-zero shell exit codes are results, not errors** — they trigger `on_result`, not `on_error` (SPEC §5.5, §16).
8. **`exit_code: any` does NOT match 0** — matches any non-zero exit code only.

## Rules

- No `unwrap()`/`expect()` — use `?` and `AilError`
- No `println!`/`eprintln!` — use `tracing::{info, warn, error}`
- `dto.rs` only: `#[derive(Deserialize)]`
- `domain.rs` only: clean domain types, no serde
- Modules returning `Result<_, AilError>` need `#[allow(clippy::result_large_err)]`

## Testing

Tests live in `ail-core/tests/spec/` with one file per SPEC section:

```
s03  — file format / YAML parsing
s04  — execution model
s05  — step specification (core fields)
s05_3 — on_result multi-branch evaluation
s05_5 — context:shell: steps + file path resolution
s08  — runner adapter
s09  — sub-pipeline execution + template vars in pipeline: paths
s09  — tool permissions (separate file: s09_tool_permissions)
s11  — template variables
s18  — materialize
s21  — MVP scope
```

`#[ignore]` tests require the `claude` binary and a live session — run with
`cargo nextest run --include-ignored` outside a Claude Code session.

Fixtures: `ail-core/tests/fixtures/` — minimal, solo_developer, invalid_* YAML variants.
