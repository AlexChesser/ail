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
    /// Pipeline-wide concurrency cap for async steps (SPEC §29.10). None = unlimited.
    pub max_concurrency: Option<u64>,
    /// Pipeline-wide sampling defaults (SPEC §30). Orthogonal to provider —
    /// applies as the lowest-precedence baseline for every step, regardless of provider.
    pub sampling: Option<SamplingDto>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProviderDto {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub read_timeout_seconds: Option<u64>,
    pub max_history_messages: Option<usize>,
    /// Sampling parameters attached to this provider (SPEC §30.2). When a step
    /// uses this provider, these sampling defaults apply with higher precedence
    /// than pipeline-wide `defaults.sampling`, but lower than per-step `sampling:`.
    pub sampling: Option<SamplingDto>,
}

/// Sampling parameters DTO (SPEC §30). Same shape is reused at three scopes:
/// pipeline defaults (`defaults.sampling`), provider-attached
/// (`defaults.provider.sampling`), and per-step (`step.sampling`).
#[derive(Debug, Default, Deserialize, Clone)]
pub struct SamplingDto {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u64>,
    pub max_tokens: Option<u64>,
    pub stop_sequences: Option<Vec<String>>,
    /// Reasoning / extended-thinking intensity. Accepts a float in [0.0, 1.0]
    /// or a YAML boolean (`true` → 1.0, `false` → 0.0).
    pub thinking: Option<ThinkingDto>,
}

/// Raw YAML value for `thinking:` — either a float or a boolean.
/// Normalized to `f64` in domain validation.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum ThinkingDto {
    Number(f64),
    Bool(bool),
}

impl ThinkingDto {
    /// Normalize to the canonical f64 representation.
    /// `true` → 1.0, `false` → 0.0, numbers pass through.
    pub fn to_f64(self) -> f64 {
        match self {
            ThinkingDto::Number(n) => n,
            ThinkingDto::Bool(true) => 1.0,
            ThinkingDto::Bool(false) => 0.0,
        }
    }
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

    // ── Parallel execution fields (SPEC §29) ───────────────────────────────────
    /// Marks this step as non-blocking. The pipeline cursor advances immediately
    /// after launching the step (SPEC §29.1).
    #[serde(rename = "async")]
    pub async_step: Option<bool>,
    /// Step IDs this step depends on. The step waits for all named dependencies
    /// to complete before executing (SPEC §29.1).
    pub depends_on: Option<Vec<String>>,

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

    /// Per-step sampling parameter override (SPEC §30). Highest-precedence scope;
    /// overrides provider-attached and pipeline-wide sampling defaults field-by-field.
    pub sampling: Option<SamplingDto>,
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
    /// Regex literal `/PATTERN/FLAGS` matched against the step's `response`
    /// (SPEC §5.4, §12.3). Shorthand for `expression: '{{ step.<id>.response }}
    /// matches /.../flags'`.
    pub matches: Option<String>,
    /// Full §12.2 condition expression (SPEC §5.4 `expression:`). Supports any
    /// operator the §12.2 grammar supports, including `matches` for regex.
    pub expression: Option<String>,
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
