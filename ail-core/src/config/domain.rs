use std::path::PathBuf;

/// Maximum depth for nested sub-pipeline calls. Prevents infinite recursion
/// when pipelines call each other in a cycle.
pub const MAX_SUB_PIPELINE_DEPTH: usize = 16;

/// Provider and model configuration resolved from pipeline defaults, per-step overrides,
/// or CLI flags. All fields are optional — unset fields fall back to runner/environment defaults.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    /// Model name to pass as `--model` to the runner (e.g. `gemma3:1b`, `claude-sonnet-4-20250514`).
    pub model: Option<String>,
    /// Provider base URL, set as `ANTHROPIC_BASE_URL` in the runner subprocess environment.
    pub base_url: Option<String>,
    /// Provider auth token, set as `ANTHROPIC_AUTH_TOKEN` in the runner subprocess environment.
    pub auth_token: Option<String>,
    /// Cost per 1000 input tokens in USD. Used for accurate cost attribution.
    /// For local/free providers (e.g., Ollama), set to 0.0.
    pub input_cost_per_1k: Option<f64>,
    /// Cost per 1000 output tokens in USD. Used for accurate cost attribution.
    /// For local/free providers (e.g., Ollama), set to 0.0.
    pub output_cost_per_1k: Option<f64>,
}

impl ProviderConfig {
    /// Merge another `ProviderConfig` on top of `self`, with `other` taking precedence.
    /// Fields present in `other` override fields in `self`; absent fields fall through.
    pub fn merge(self, other: ProviderConfig) -> ProviderConfig {
        ProviderConfig {
            model: other.model.or(self.model),
            base_url: other.base_url.or(self.base_url),
            auth_token: other.auth_token.or(self.auth_token),
            input_cost_per_1k: other.input_cost_per_1k.or(self.input_cost_per_1k),
            output_cost_per_1k: other.output_cost_per_1k.or(self.output_cost_per_1k),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub steps: Vec<Step>,
    pub source: Option<PathBuf>,
    /// Default provider/model config applied to all steps unless overridden (SPEC §3, §15).
    pub defaults: ProviderConfig,
    /// Pipeline-wide tool policy applied to steps that declare no per-step `tools:` (SPEC §3.2).
    /// Per-step tools override entirely — if a step declares any tools, the default is ignored.
    pub default_tools: Option<ToolPolicy>,
}

impl Pipeline {
    /// Default pipeline used when no `.ail.yaml` is found (SPEC §3.1, §4.1).
    /// Contains only the implicit `invocation` step, which represents the triggering
    /// event (the `--once` prompt) and is populated by the host before `execute()` runs.
    pub fn passthrough() -> Self {
        Pipeline {
            steps: vec![Step {
                id: StepId("invocation".to_string()),
                body: StepBody::Prompt("{{ session.invocation_prompt }}".to_string()),
                message: None,
                tools: None,
                on_result: None,
                model: None,
                runner: None,
            }],
            source: None,
            defaults: ProviderConfig::default(),
            default_tools: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: StepId,
    pub body: StepBody,
    /// Optional human-readable message shown to the operator when this step's HITL gate is reached.
    /// Only meaningful for steps with `action: pause_for_human`.
    pub message: Option<String>,
    /// Pre-approved and pre-denied tools for this step (SPEC §5.8).
    /// Passed as `--allowedTools` / `--disallowedTools` to the runner.
    pub tools: Option<ToolPolicy>,
    /// Declarative branching after step completion (SPEC §5.4).
    pub on_result: Option<Vec<ResultBranch>>,
    /// Per-step model override. Overrides `pipeline.defaults.model` but not CLI flags.
    pub model: Option<String>,
    /// Optional runner name override for this step (SPEC §19).
    /// Selection hierarchy: per-step `runner:` → `AIL_DEFAULT_RUNNER` env → `"claude"`.
    pub runner: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ToolPolicy {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StepId(pub String);

impl StepId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub enum StepBody {
    Prompt(String),
    Skill(PathBuf),
    /// Path to a sub-pipeline YAML file. May contain `{{ variable }}` syntax;
    /// the path is template-resolved at execution time (SPEC §11).
    SubPipeline(String),
    Action(ActionKind),
    Context(ContextSource),
}

#[derive(Debug, Clone)]
pub enum ContextSource {
    Shell(String),
}

#[derive(Debug, Clone)]
pub enum ActionKind {
    PauseForHuman,
}

/// One branch in an `on_result` multi-branch array (SPEC §5.4).
#[derive(Debug, Clone)]
pub struct ResultBranch {
    pub matcher: ResultMatcher,
    pub action: ResultAction,
}

#[derive(Debug, Clone)]
pub enum ResultMatcher {
    Contains(String),
    ExitCode(ExitCodeMatch),
    Always,
}

/// Matches process exit codes in `on_result` branches.
/// `Any` matches any non-zero exit code. Does not match 0.
#[derive(Debug, Clone)]
pub enum ExitCodeMatch {
    Exact(i32),
    Any,
}

#[derive(Debug, Clone)]
pub enum ResultAction {
    Continue,
    Break,
    AbortPipeline,
    PauseForHuman,
    /// Conditionally call another pipeline. Path may contain `{{ variable }}` syntax;
    /// resolved at execution time. Follows the sub-pipeline isolation model (SPEC §9).
    Pipeline(String),
}
