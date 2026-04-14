use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Default, Deserialize)]
pub struct PipelineFileDto {
    pub version: Option<String>,
    /// `FROM` field for pipeline inheritance (SPEC §7).
    /// Accepts file paths only — relative, absolute, or home-relative.
    #[serde(rename = "FROM")]
    pub from: Option<String>,
    pub defaults: Option<DefaultsDto>,
    pub pipeline: Option<Vec<StepDto>>,
    /// Named pipeline definitions (SPEC §10). Maps pipeline name → step list.
    /// Steps within named pipelines use the same DTO schema as the main pipeline.
    pub pipelines: Option<HashMap<String, Vec<StepDto>>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DefaultsDto {
    pub model: Option<String>,
    pub provider: Option<ProviderDto>,
    pub timeout_seconds: Option<u64>,
    pub tools: Option<ToolsDto>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProviderDto {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub read_timeout_seconds: Option<u64>,
    pub max_history_messages: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StepDto {
    pub id: Option<String>,
    pub prompt: Option<String>,
    pub skill: Option<String>,
    pub pipeline: Option<String>,
    pub action: Option<String>,
    /// Optional human-readable message shown in the HITL gate banner when `action: pause_for_human`
    /// or `action: modify_output`.
    pub message: Option<String>,
    /// Headless-mode behavior for HITL gates (`skip`, `abort`, `use_default`). Defaults to `skip`.
    pub on_headless: Option<String>,
    /// Default value used when `on_headless: use_default` and no human is available.
    pub default_value: Option<String>,
    pub context: Option<ContextDto>,
    pub tools: Option<ToolsDto>,
    pub on_result: Option<OnResultDto>,
    pub model: Option<String>,
    /// Optional runner name override for this step. Overrides `AIL_DEFAULT_RUNNER` and the
    /// pipeline-level default. See §19 and `RunnerFactory`.
    pub runner: Option<String>,
    /// Optional condition controlling whether this step executes (SPEC §12).
    /// Supported values: `"always"` (default), `"never"`.
    pub condition: Option<String>,
    /// Optional list of system prompt additions for this step (SPEC §5.9).
    pub append_system_prompt: Option<Vec<AppendSystemPromptEntryDto>>,
    /// Optional system prompt override for this step (SPEC §5.9).
    /// When set, replaces the runner's default system prompt entirely.
    pub system_prompt: Option<String>,
    /// Whether this step should resume the previous runner session (SPEC §15.4).
    /// Defaults to `false` — each step starts a fresh session.
    pub resume: Option<bool>,
    /// Hook operation: insert this step immediately before the named step (SPEC §7.2).
    pub run_before: Option<String>,
    /// Hook operation: insert this step immediately after the named step (SPEC §7.2).
    pub run_after: Option<String>,
    /// Hook operation: replace the named step with this step's body (SPEC §7.2).
    #[serde(rename = "override")]
    pub override_step: Option<String>,
    /// Hook operation: remove the named step entirely (SPEC §7.2).
    /// This is a bare string field — no step body is needed.
    pub disable: Option<String>,
    /// Error handling strategy for this step (SPEC §16).
    /// Supported values: `"continue"`, `"retry"`, `"abort_pipeline"`.
    /// Defaults to `abort_pipeline` when not specified.
    pub on_error: Option<String>,
    /// Maximum number of retries when `on_error: retry` is set.
    /// Required when `on_error` is `"retry"`, ignored otherwise.
    pub max_retries: Option<u32>,
    /// Private pre-processing steps that run before this step's prompt fires (SPEC §5.10).
    pub before: Option<Vec<ChainStepDto>>,
    /// Private post-processing steps chained to this step (SPEC §5.7).
    pub then: Option<Vec<ChainStepDto>>,

    // ── Reserved v0.3 fields ─────────────────────────────────────────────────
    // Accepted by serde so users get a clear validation error instead of
    // "unknown field". Rejected at validation time until implemented.
    /// Bounded repeat-until loop (SPEC §27).
    pub do_while: Option<DoWhileDto>,
    /// Collection iteration (SPEC §28).
    pub for_each: Option<ForEachDto>,
    /// Reserved: JSON Schema for step output validation (SPEC §26). Rejected at validation time.
    pub output_schema: Option<serde_json::Value>,
    /// Reserved: JSON Schema for step input validation (SPEC §26). Rejected at validation time.
    pub input_schema: Option<serde_json::Value>,
}

/// A step entry in a `before:` or `then:` chain (SPEC §5.7, §5.10).
///
/// Supports both short-form (bare string: skill reference or prompt file path)
/// and full-form (a full step block minus `id`, `condition`, `on_result`).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ChainStepDto {
    /// Short-form: bare string — skill reference or prompt file path.
    Short(String),
    /// Full-form: step block with optional fields.
    Full(Box<StepDto>),
}

#[derive(Debug, Default, Deserialize)]
pub struct ContextDto {
    pub shell: Option<String>,
}

/// DTO for `do_while:` bounded repeat-until loop (SPEC §27).
#[derive(Debug, Default, Deserialize)]
pub struct DoWhileDto {
    /// Maximum number of iterations (required, must be ≥ 1).
    pub max_iterations: Option<u64>,
    /// Condition expression evaluated after each iteration; loop exits when true.
    pub exit_when: Option<String>,
    /// Inner steps executed each iteration. Mutually exclusive with `pipeline`.
    pub steps: Option<Vec<StepDto>>,
    /// Path to an external pipeline file whose steps become the loop body (SPEC §27.2).
    /// Mutually exclusive with `steps`.
    pub pipeline: Option<String>,
}

/// DTO for `for_each:` collection iteration (SPEC §28).
#[derive(Debug, Default, Deserialize)]
pub struct ForEachDto {
    /// Template expression resolving to a validated array (e.g. `{{ step.plan.items }}`).
    pub over: Option<String>,
    /// Local name for the current item within the loop body. Defaults to `item`.
    #[serde(rename = "as")]
    pub as_name: Option<String>,
    /// Hard cap on items processed. Items beyond this limit are not processed.
    pub max_items: Option<u64>,
    /// What happens when the array contains more items than `max_items`.
    pub on_max_items: Option<String>,
    /// Inner steps executed once per item. Mutually exclusive with `pipeline`.
    pub steps: Option<Vec<StepDto>>,
    /// Path to an external pipeline file whose steps become the loop body (SPEC §28.2).
    /// Mutually exclusive with `steps`.
    pub pipeline: Option<String>,
}

/// `on_result:` accepts two shapes (SPEC §5.4, §26.4):
///
/// 1. Multi-branch array: `on_result: [{ contains: ..., action: ... }, ...]`
/// 2. Field-equals binary branch: `on_result: { field: ..., equals: ..., if_true: ..., if_false: ... }`
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum OnResultDto {
    /// Standard multi-branch array of matchers.
    Branches(Vec<OnResultBranchDto>),
    /// Field-equals binary branch with `if_true` / `if_false` (SPEC §26.4).
    FieldEquals(Box<FieldEqualsDto>),
}

#[derive(Debug, Default, Deserialize)]
pub struct OnResultBranchDto {
    pub contains: Option<String>,
    pub exit_code: Option<ExitCodeDto>,
    pub always: Option<bool>,
    pub action: Option<String>,
    /// Optional prompt override passed to the child session when action is `pipeline:`.
    /// Template variables are resolved at execution time (SPEC §9).
    pub prompt: Option<String>,
}

/// DTO for the `field:` + `equals:` binary branching format of `on_result` (SPEC §26.4).
#[derive(Debug, Deserialize)]
pub struct FieldEqualsDto {
    pub field: String,
    pub equals: serde_json::Value,
    pub if_true: FieldEqualsActionDto,
    pub if_false: Option<FieldEqualsActionDto>,
}

/// Action declaration inside a `field:` + `equals:` branch.
#[derive(Debug, Deserialize)]
pub struct FieldEqualsActionDto {
    pub action: String,
    pub message: Option<String>,
    pub prompt: Option<String>,
}

/// Handles both `exit_code: 0` (integer) and `exit_code: any` (string).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ExitCodeDto {
    Integer(i32),
    Keyword(String),
}

#[derive(Debug, Deserialize)]
pub struct ToolsDto {
    /// When `true`, passes `--tools ""` to the runner — disables all tools for this step.
    /// Overrides `allow` and `deny` if set. SPEC §5.8.
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AppendSystemPromptEntryDto {
    /// Bare string shorthand for text entry.
    Text(String),
    /// Structured entry with explicit type key.
    Structured(AppendSystemPromptStructuredDto),
}

#[derive(Debug, Deserialize)]
pub struct AppendSystemPromptStructuredDto {
    pub text: Option<String>,
    pub file: Option<String>,
    pub shell: Option<String>,
}
