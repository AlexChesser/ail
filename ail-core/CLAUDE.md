# ail-core — Library Crate

All business logic lives here. No CLI, no user interaction.
Consumed by `ail` (the binary) and future language-server / SDK targets.

## Module Responsibilities

| Module | Responsibility |
|---|---|
| `config/discovery.rs` | Walk the four-step file resolution order (SPEC §3.1) |
| `config/dto.rs` | Serde-deserialised raw structs — derives `Deserialize` |
| `config/domain.rs` | Validated domain types — no `Deserialize` derives |
| `config/validation/mod.rs` | `validate()` entry point, `cfg_err!` macro, `tools_to_policy` helper |
| `config/validation/step_body.rs` | `parse_step_body()` — primary field count check + body construction (including `parse_do_while_body`, `parse_for_each_body`, `load_loop_pipeline_steps`) |
| `config/validation/on_result.rs` | `parse_result_branches()` — DTO → domain for result matchers and actions (incl. `expression:` via `parse_condition_expression`, and `matches:` desugared to expression form) |
| `config/validation/regex_literal.rs` | `parse_regex_literal()` — parses `/PATTERN/FLAGS` into a compiled `regex::Regex` with source preservation (SPEC §12.3) |
| `config/validation/system_prompt.rs` | `parse_append_system_prompt()` — DTO → domain for system prompt entries |
| `config/validation/sampling.rs` | `validate_sampling()` — DTO → domain with range checks; normalizes thinking (f64 OR bool) to `Option<f64>` (SPEC §30.6.1) |
| `config/inheritance.rs` | FROM inheritance — path resolution, cycle detection, DTO merging, hook operations (SPEC §7, §8) |
| `config/mod.rs` | `load(path)` public entry point |
| `error.rs` | `AilError`, `ErrorContext`, `error_types` string constants |
| `executor/mod.rs` | `execute(&mut Session, &dyn Runner)` — SPEC §4.2 core invariant |
| `executor/core.rs` | `StepObserver` trait, `NullObserver`, `execute_core()`, `execute_do_while()`, `execute_for_each()` — shared step-dispatch loop + loop execution (SPEC §27, §28) |
| `executor/headless.rs` | `execute()` — headless mode entry point using `NullObserver` |
| `executor/controlled.rs` | `execute_with_control()` — TUI-controlled mode with `ChannelObserver` |
| `executor/events.rs` | `ExecuteOutcome`, `ExecutionControl`, `ExecutorEvent` |
| `executor/parallel.rs` | `ConcurrencySemaphore`, `BranchResult`, `merge_join_results()`, timestamp helpers — SPEC §29 parallel step dispatch primitives |
| `executor/helpers/mod.rs` | Re-exports all helper functions for the executor |
| `executor/helpers/invocation.rs` | `run_invocation_step()` — host-managed invocation step lifecycle |
| `executor/helpers/runner_resolution.rs` | `resolve_step_provider()`, `resolve_step_sampling()` (SPEC §30.3 three-scope merge), `build_step_runner_box()`, `resolve_effective_runner_name()` |
| `executor/helpers/shell.rs` | `run_shell_command()` — `/bin/sh -c` subprocess execution |
| `executor/helpers/condition.rs` | `evaluate_condition()` — runtime condition expression evaluation against session state (SPEC §12) |
| `executor/helpers/on_result.rs` | `evaluate_on_result()`, `build_tool_policy()` — on_result branch evaluation + tool policy |
| `executor/helpers/system_prompt.rs` | `resolve_step_system_prompts()`, `resolve_prompt_file()` — system prompt resolution and file loading |
| `executor/dispatch/mod.rs` | Re-exports step-type dispatch modules |
| `executor/dispatch/prompt.rs` | Prompt step dispatch — template resolution, runner invocation, TurnEntry construction |
| `executor/dispatch/skill.rs` | Skill step dispatch — registry lookup, template resolution, runner invocation |
| `executor/dispatch/context.rs` | Context shell step dispatch — shell execution and TurnEntry construction |
| `executor/dispatch/sub_pipeline.rs` | Sub-pipeline dispatch — recursion, depth guard, child session creation |
| `materialize.rs` | `materialize(&Pipeline) → String` — annotated YAML round-trip |
| `runner/mod.rs` | `Runner` trait, `RunResult`, `InvokeOptions` |
| `runner/subprocess.rs` | `SubprocessSession` — generic CLI subprocess lifecycle (spawn, stderr drain, cancel watchdog, reap); shared by all CLI-based runners |
| `runner/claude/mod.rs` | `ClaudeCliRunner` — orchestrates subprocess + decoder + permission listener |
| `runner/claude/decoder.rs` | `ClaudeNdjsonDecoder` — stateful NDJSON stream decoder, no process coupling; unit-testable with raw byte strings |
| `runner/claude/permission.rs` | `ClaudePermissionListener` — RAII guard for the tool-permission socket (hook settings file, accept loop, `__close__` sentinel, cleanup on drop) |
| `runner/codex/mod.rs` | `CodexRunner` — orchestrates subprocess + decoder for `codex exec --json` |
| `runner/codex/decoder.rs` | `CodexNdjsonDecoder` — stateful item-lifecycle NDJSON decoder; unit-testable with raw byte strings |
| `runner/codex/wire_dto.rs` | Serde DTOs for the `codex exec --json` wire format |
| `runner/factory.rs` | `RunnerFactory` — builds runners by name; honours `AIL_DEFAULT_RUNNER` env; checks plugin registry for unknown names |
| `runner/plugin/mod.rs` | Plugin system entry point — re-exports `PluginRegistry`, `PluginManifest`, `ProtocolRunner` |
| `runner/plugin/discovery.rs` | `discover_plugins()` — scans `~/.ail/runners/` for manifest files; returns `PluginRegistry` |
| `runner/plugin/jsonrpc.rs` | JSON-RPC 2.0 wire types (request, response, notification, method/notification constants) |
| `runner/plugin/manifest_dto.rs` | Serde DTO for runner manifest YAML/JSON |
| `runner/plugin/manifest.rs` | Validated domain type for plugin manifests — no serde |
| `runner/plugin/validation.rs` | DTO → domain validation for manifests |
| `runner/plugin/protocol_runner.rs` | `ProtocolRunner` — generic `Runner` impl that speaks JSON-RPC to any compliant executable |
| `runner/http.rs` | `HttpRunner` — direct OpenAI-compatible HTTP runner (Ollama, direct API); full system-prompt control, think flag, in-memory session continuity |
| `runner/dry_run.rs` | `DryRunRunner` — production no-op runner for `--dry-run` mode; returns synthetic response, zero tokens, zero cost |
| `runner/stub.rs` | `StubRunner`, `CountingStubRunner`, `EchoStubRunner`, `RecordingStubRunner`, `SequenceStubRunner` — deterministic test doubles |
| `session/log_provider.rs` | `LogProvider` trait + `JsonlProvider` (NDJSON) + `NullProvider` (tests) |
| `session/state.rs` | `Session` — `run_id`, `pipeline`, `invocation_prompt`, `turn_log` |
| `session/turn_log.rs` | `TurnLog` — in-memory entry store + delegates persistence to `LogProvider` |
| `skill.rs` | `SkillRegistry` — built-in skill resolution; `SkillDefinition` data type; maps `ail/<name>` → prompt template |
| `template.rs` | `resolve(template, &Session) → Result<String, AilError>` |

## Key Types

```rust
// Pipeline and its steps
pub struct Pipeline { pub steps: Vec<Step>, pub source: Option<PathBuf>, pub defaults: ProviderConfig, pub timeout_seconds: Option<u64>, pub default_tools: Option<ToolPolicy>, pub named_pipelines: HashMap<String, Vec<Step>>, pub max_concurrency: Option<u64>, pub sampling_defaults: Option<SamplingConfig> }
// default_tools: pipeline-wide fallback; per-step tools override entirely (SPEC §3.2)
// named_pipelines: named pipeline definitions from the `pipelines:` section (SPEC §10)
// sampling_defaults: pipeline-wide sampling baseline (SPEC §30.2); orthogonal to provider-attached sampling on `defaults: ProviderConfig`
pub struct Step    { pub id: StepId, pub body: StepBody, pub message: Option<String>, pub tools: Option<ToolPolicy>, pub on_result: Option<Vec<ResultBranch>>, pub model: Option<String>, pub runner: Option<String>, pub condition: Option<Condition>, pub append_system_prompt: Option<Vec<SystemPromptEntry>>, pub system_prompt: Option<String>, pub resume: bool, pub async_step: bool, pub depends_on: Vec<StepId>, pub on_error: Option<OnError>, pub before: Vec<Step>, pub then: Vec<Step>, pub output_schema: Option<serde_json::Value>, pub input_schema: Option<serde_json::Value>, pub sampling: Option<SamplingConfig> }
// async_step / depends_on: parallel execution primitives (SPEC §29). async_step=true marks a non-blocking step; depends_on lists step IDs this step waits for.
// before: private pre-processing chain (SPEC §5.10) — runs before the step fires
// then: private post-processing chain (SPEC §5.7) — runs after the step completes
// output_schema: optional JSON Schema for validating step output (SPEC §26.1); validated at parse time, response validated at runtime
// input_schema: optional JSON Schema for validating preceding step's output (SPEC §26.2); validated at parse time, runtime validation before step executes
pub enum Condition { Always, Never, Expression(ConditionExpr), Regex(RegexCondition) }  // SPEC §12 — None means Always; Never skips; Expression evaluates comparison at runtime; Regex evaluates regex::is_match at runtime (SPEC §12.2/§12.3). Does NOT derive PartialEq — regex::Regex doesn't implement it.
pub struct ConditionExpr { pub lhs: String, pub op: ConditionOp, pub rhs: String }
pub struct RegexCondition { pub lhs: String, pub regex: regex::Regex, pub source: String }  // SPEC §12.3 — regex compiled at parse time via parse_regex_literal(); source is original /PATTERN/FLAGS for diagnostics
pub enum ConditionOp { Eq, Ne, Contains, StartsWith, EndsWith }
pub enum OnError { Continue, Retry { max_retries: u32 }, AbortPipeline }  // SPEC §16 — None means AbortPipeline (default)
pub enum StepBody  { Prompt(String), Skill { name: String }, SubPipeline { path: String, prompt: Option<String> }, NamedPipeline { name: String, prompt: Option<String> }, Action(ActionKind), Context(ContextSource), DoWhile { max_iterations, exit_when, steps }, ForEach { over, as_name, max_items, on_max_items, steps } }
// NamedPipeline: references a named pipeline defined in pipelines: section (SPEC §10)
// NamedPipeline.prompt: when Some, overrides child session's invocation_prompt (same as SubPipeline)
// SubPipeline.path may contain {{ variable }} syntax — resolved at execution time (SPEC §11)
// SubPipeline.prompt: when Some, overrides child session's invocation_prompt instead of using parent's last_response (SPEC §9.3)
pub enum ContextSource { Shell(String) }
pub enum ActionKind { PauseForHuman, ModifyOutput { headless_behavior: HitlHeadlessBehavior, default_value: Option<String> }, Join { on_error_mode: JoinErrorMode } }
pub enum JoinErrorMode { FailFast, WaitForAll }  // SPEC §29.7 — default FailFast
pub enum HitlHeadlessBehavior { Skip, Abort, UseDefault }
pub struct ResultBranch { pub matcher: ResultMatcher, pub action: ResultAction }
pub enum ResultMatcher { Contains(String), ExitCode(ExitCodeMatch), Field { name: String, equals: serde_json::Value }, Expression { source: String, condition: Condition }, Always }
// Field: exact equality match against a named field in validated input JSON (SPEC §26.4); requires input_schema
// Expression: §12.2 condition grammar (SPEC §5.4 `expression:`) — source is original expression string for materialize/diagnostics; condition is Expression(ConditionExpr) or Regex(RegexCondition). Named `matches:` in YAML desugars to Expression at parse time (SPEC §5.4).
pub enum ExitCodeMatch { Exact(i32), Any }
pub enum ResultAction { Continue, Break, AbortPipeline, PauseForHuman, Pipeline { path: String, prompt: Option<String> } }
// Pipeline.path may contain {{ variable }} syntax — resolved at execution time (SPEC §11)
// Pipeline.prompt: when Some, overrides child session's invocation_prompt instead of using parent's last_response (SPEC §9.3)
// const MAX_SUB_PIPELINE_DEPTH: usize = 16 — enforced by execute_inner depth counter

// Provider/model config (SPEC §15) — resolved chain: defaults → per-step → cli_provider
pub struct ProviderConfig { pub model: Option<String>, pub base_url: Option<String>, pub auth_token: Option<String>, pub connect_timeout_seconds: Option<u64>, pub read_timeout_seconds: Option<u64>, pub max_history_messages: Option<usize>, pub sampling: Option<SamplingConfig> }
// merge(self, other): other wins on conflict; absent fields fall through from self; sampling merges field-wise (not replaced)
// connect_timeout_seconds / read_timeout_seconds: HTTP runner timeouts (defaults 10s / 300s)
// max_history_messages: sliding window cap for HTTP runner session history (None = unlimited)
// sampling: provider-attached sampling defaults (SPEC §30.2); middle precedence between pipeline defaults and per-step

// Sampling parameter config (SPEC §30)
pub struct SamplingConfig { pub temperature: Option<f64>, pub top_p: Option<f64>, pub top_k: Option<u64>, pub max_tokens: Option<u64>, pub stop_sequences: Option<Vec<String>>, pub thinking: Option<f64> }
// merge(self, other): field-level, other wins; stop_sequences replaces (no append — SPEC §30.3.1)
// is_empty(): true when all fields are None — used by resolve_step_sampling to return None rather than empty config
// thinking: [0.0, 1.0] — YAML accepts bool aliases (true→1.0, false→0.0) via ThinkingDto at the DTO layer
// Each runner quantizes thinking to its own granularity: ClaudeCLI quartiles → --effort; HTTP threshold 0.5 → bool

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
pub struct InvokeOptions { pub resume_session_id: Option<String>, pub tool_policy: ToolPermissionPolicy, pub model: Option<String>, pub extensions: Option<Box<dyn Any + Send>>, pub permission_responder: Option<PermissionResponder>, pub cancel_token: Option<CancelToken>, pub system_prompt: Option<String>, pub append_system_prompt: Vec<String>, pub sampling: Option<SamplingConfig> }
// sampling: effective sampling config after the three-scope merge — populated by resolve_step_sampling() (SPEC §30.3)
// extensions: runners downcast to their own type (e.g. ClaudeInvokeExtensions { base_url, auth_token, permission_socket }).
// cancel_token: event-driven cancellation — CancelToken wraps Arc<AtomicBool> + Arc<event_listener::Event>.
// Runners block on token.listen().wait() (no polling). Callers signal via token.cancel().
// permission_responder: when set, the runner intercepts tool permission requests and calls this callback.
// system_prompt / append_system_prompt: per-step system prompt override and extensions (SPEC §5.9).

// Claude CLI runner config (runner/claude.rs) — builder for ClaudeCliRunner
pub struct ClaudeCliRunnerConfig { pub claude_bin: String, pub headless: bool }
// impl Default: claude_bin="claude", headless=false
// Builder: .headless(bool), .claude_bin(str), .build() -> ClaudeCliRunner
// ClaudeCliRunner::from_config(config) is the preferred constructor

// Claude CLI runner extensions (runner/claude.rs)
pub struct ClaudeInvokeExtensions { pub base_url: Option<String>, pub auth_token: Option<String>, pub permission_socket: Option<PathBuf> }

// HTTP runner config (runner/http.rs) — calls any OpenAI-compatible /v1/chat/completions endpoint
pub struct HttpRunnerConfig { pub base_url: String, pub auth_token: Option<String>, pub default_model: Option<String>, pub think: Option<bool>, pub connect_timeout_seconds: Option<u64>, pub read_timeout_seconds: Option<u64>, pub max_history_messages: Option<usize> }
// Default base_url: "http://localhost:11434/v1" (local Ollama)
// think: Some(false) disables extended thinking for qwen3 and similar models
// HttpRunner::new(config, store) — takes explicit HttpSessionStore for shared session state
// HttpRunner::ollama(model, store) convenience ctor: base_url=localhost:11434, think=Some(false)
// Factory names: "http" or "ollama" — reads AIL_HTTP_BASE_URL, AIL_HTTP_TOKEN, AIL_HTTP_MODEL, AIL_HTTP_THINK
// Session continuity: shared in-memory store scoped to pipeline run via Session.http_session_store
// Session IDs are NOT resumable across process restarts — correlation tokens only
pub type HttpSessionStore = Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>;
// All HttpRunner instances sharing the same store see each other's sessions

// Executor outcome
pub enum ExecuteOutcome { Completed, Break { step_id: String } }

// Session
pub struct Session { pub run_id: String, pub pipeline: Pipeline, pub invocation_prompt: String, pub turn_log: TurnLog, pub cli_provider: ProviderConfig, pub cwd: String, pub runner_name: String, pub headless: bool, pub http_session_store: HttpSessionStore, pub do_while_context: Option<DoWhileContext>, pub for_each_context: Option<ForEachContext>, pub loop_depth: usize }
// cwd: captured at Session::new() time via std::env::current_dir(); used by {{ session.cwd }} template variable.
// do_while_context: set during do_while loop body execution, enables {{ do_while.iteration }} and {{ do_while.max_iterations }} template variables
// for_each_context: set during for_each loop body execution, enables {{ for_each.item }}/{{ for_each.<as_name> }}, {{ for_each.index }}, {{ for_each.total }} template variables
// loop_depth: current nesting depth of loop constructs (do_while, for_each), checked against MAX_LOOP_DEPTH (8)
// Note: the "Allow for session" tool allowlist (SPEC §13.2, §13.4) lives in the `ail` binary crate
// (control_bridge::AllowlistArc), not on Session — session allowlisting is a binary-layer concern.
// TurnEntry carries prompt-step fields (response, runner_session_id, thinking, tool_events) and context-step fields (stdout, stderr, exit_code)
// tool_events: Vec<ToolEvent> — populated from RunResult.tool_events for prompt steps; empty for context/action/sub-pipeline steps
pub struct DoWhileContext { pub loop_id: String, pub iteration: u64, pub max_iterations: u64 }
// DoWhileContext: active loop context — set during do_while body execution, cleared after loop exits (SPEC §27)
pub struct ForEachContext { pub loop_id: String, pub index: u64, pub total: u64, pub item: String, pub as_name: String }
// ForEachContext: active loop context — set during for_each body execution, cleared after loop exits (SPEC §28)
pub enum OnMaxItems { Continue, AbortPipeline }
// OnMaxItems: behavior when for_each array exceeds max_items (SPEC §28.2)

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
| `RUNNER_CANCELLED` | `ail:runner/cancelled` |
| `RUNNER_NOT_FOUND` | `ail:runner/not-found` |
| `PIPELINE_ABORTED` | `ail:pipeline/aborted` |
| `STORAGE_QUERY_FAILED` | `ail:storage/query-failed` |
| `RUN_NOT_FOUND` | `ail:storage/run-not-found` |
| `STORAGE_DELETE_FAILED` | `ail:storage/delete-failed` |
| `PLUGIN_MANIFEST_INVALID` | `ail:plugin/manifest-invalid` |
| `PLUGIN_SPAWN_FAILED` | `ail:plugin/spawn-failed` |
| `PLUGIN_PROTOCOL_ERROR` | `ail:plugin/protocol-error` |
| `PLUGIN_TIMEOUT` | `ail:plugin/timeout` |
| `CONDITION_INVALID` | `ail:condition/invalid` |
| `PIPELINE_CIRCULAR_REFERENCE` | `ail:pipeline/circular-reference` |
| `CIRCULAR_INHERITANCE` | `ail:config/circular-inheritance` |
| `SKILL_UNKNOWN` | `ail:skill/unknown` |
| `DO_WHILE_MAX_ITERATIONS` | `ail:do-while/max-iterations-exceeded` |
| `LOOP_DEPTH_EXCEEDED` | `ail:loop/depth-exceeded` |
| `OUTPUT_SCHEMA_VALIDATION_FAILED` | `ail:schema/output-validation-failed` |
| `INPUT_SCHEMA_VALIDATION_FAILED` | `ail:schema/input-validation-failed` |
| `SCHEMA_COMPATIBILITY_FAILED` | `ail:schema/compatibility-failed` |
| `FOR_EACH_SOURCE_INVALID` | `ail:for-each/source-invalid` |

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
- **Use `..Default::default()` for struct construction** — when building `Step`, `TurnEntry`, or other structs with many optional/defaultable fields, set only the fields that differ from the default and use `..Default::default()` for the rest. Never enumerate every field with `None`/`0`/`vec![]` explicitly. This prevents breakage when new fields are added.
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
s05_7 — before:/then: step chains (§5.7, §5.10)
s06  — skills
s07  — pipeline inheritance (FROM)
s08  — runner adapter
s09  — sub-pipeline execution + template vars in pipeline: paths
s09  — tool permissions (separate file: s09_tool_permissions)
s10  — named pipelines (definition, reference, execution, circular detection, materialize)
s11  — template variables
s12  — step conditions (always, never, expression)
s16  — on_error error handling (continue, retry, abort_pipeline)
s18  — materialize
s21  — MVP scope
s26  — output_schema, input_schema, field:equals:, schema compatibility
s27  — do_while: bounded repeat-until loop (parse + executor)
s28  — for_each: collection iteration (parse + executor)
s26_s27_s28_integration — cross-feature integration tests (schema+loops, nested loops, full pipeline)
s30  — sampling parameter control (parse + merge + executor flow, SPEC §30)
```

`#[ignore]` tests require the `claude` binary and a live session — run with
`cargo nextest run --include-ignored` outside a Claude Code session.

Fixtures: `ail-core/tests/fixtures/` — minimal, solo_developer, invalid_* YAML variants.
