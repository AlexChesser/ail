use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct PipelineFileDto {
    pub version: Option<String>,
    pub defaults: Option<DefaultsDto>,
    pub pipeline: Option<Vec<StepDto>>,
    /// Named pipeline definitions (SPEC §10). Maps pipeline name → step list.
    /// Steps within named pipelines use the same DTO schema as the main pipeline.
    pub pipelines: Option<HashMap<String, Vec<StepDto>>>,
}

#[derive(Deserialize)]
pub struct DefaultsDto {
    pub model: Option<String>,
    pub provider: Option<ProviderDto>,
    pub timeout_seconds: Option<u64>,
    pub tools: Option<ToolsDto>,
}

#[derive(Deserialize)]
pub struct ProviderDto {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub read_timeout_seconds: Option<u64>,
    pub max_history_messages: Option<usize>,
}

#[derive(Deserialize)]
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
    pub on_result: Option<Vec<OnResultBranchDto>>,
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
}

#[derive(Deserialize)]
pub struct ContextDto {
    pub shell: Option<String>,
}

#[derive(Deserialize)]
pub struct OnResultBranchDto {
    pub contains: Option<String>,
    pub exit_code: Option<ExitCodeDto>,
    pub always: Option<bool>,
    pub action: Option<String>,
    /// Optional prompt override passed to the child session when action is `pipeline:`.
    /// Template variables are resolved at execution time (SPEC §9).
    pub prompt: Option<String>,
}

/// Handles both `exit_code: 0` (integer) and `exit_code: any` (string).
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ExitCodeDto {
    Integer(i32),
    Keyword(String),
}

#[derive(Deserialize)]
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
