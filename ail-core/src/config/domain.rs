use std::collections::HashMap;
use std::path::PathBuf;

/// One entry in an `append_system_prompt:` array (SPEC §5.9).
#[derive(Debug, Clone, PartialEq)]
pub enum SystemPromptEntry {
    /// Inline text, appended verbatim.
    Text(String),
    /// Path to a file whose contents are read at step runtime.
    File(PathBuf),
    /// Shell command whose stdout+stderr output is injected at step runtime.
    Shell(String),
}

/// Maximum depth for nested sub-pipeline calls. Prevents infinite recursion
/// when pipelines call each other in a cycle.
pub const MAX_SUB_PIPELINE_DEPTH: usize = 16;

/// Maximum nesting depth for loop constructs (`do_while`, `for_each`).
/// Prevents runaway resource consumption from deeply nested loops (SPEC §27).
pub const MAX_LOOP_DEPTH: usize = 8;

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
    /// Connect timeout in seconds for HTTP runner. `None` uses runner default (10s).
    pub connect_timeout_seconds: Option<u64>,
    /// Read timeout in seconds for HTTP runner. `None` uses runner default (300s).
    pub read_timeout_seconds: Option<u64>,
    /// Maximum number of non-system messages in HTTP runner session history.
    /// Older messages are dropped (sliding window). `None` means unlimited.
    pub max_history_messages: Option<usize>,
}

impl ProviderConfig {
    /// Merge another `ProviderConfig` on top of `self`, with `other` taking precedence.
    /// Fields present in `other` override fields in `self`; absent fields fall through.
    pub fn merge(self, other: ProviderConfig) -> ProviderConfig {
        ProviderConfig {
            model: other.model.or(self.model),
            base_url: other.base_url.or(self.base_url),
            auth_token: other.auth_token.or(self.auth_token),
            connect_timeout_seconds: other
                .connect_timeout_seconds
                .or(self.connect_timeout_seconds),
            read_timeout_seconds: other.read_timeout_seconds.or(self.read_timeout_seconds),
            max_history_messages: other.max_history_messages.or(self.max_history_messages),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Pipeline {
    pub steps: Vec<Step>,
    pub source: Option<PathBuf>,
    /// Default provider/model config applied to all steps unless overridden (SPEC §3, §15).
    pub defaults: ProviderConfig,
    /// Optional per-run timeout in seconds declared in `defaults.timeout_seconds` (SPEC §3.2).
    /// Parsed but not yet enforced at runtime — available for future scheduler use.
    pub timeout_seconds: Option<u64>,
    /// Pipeline-wide tool policy applied to steps that declare no per-step `tools:` (SPEC §3.2).
    /// Per-step tools override entirely — if a step declares any tools, the default is ignored.
    pub default_tools: Option<ToolPolicy>,
    /// Named pipeline definitions (SPEC §10). Maps pipeline name → list of validated steps.
    /// Referenced by `StepBody::NamedPipeline` steps at execution time.
    pub named_pipelines: HashMap<String, Vec<Step>>,
}

impl Pipeline {
    /// Default pipeline used when no `.ail.yaml` is found (SPEC §3.1, §4.1).
    /// Contains only the implicit `invocation` step, which represents the triggering
    /// event (the `--once` prompt) and is populated by the host before `execute()` runs.
    pub fn passthrough() -> Self {
        Pipeline {
            steps: vec![Step {
                id: StepId("invocation".to_string()),
                body: StepBody::Prompt("{{ step.invocation.prompt }}".to_string()),
                message: None,
                tools: None,
                on_result: None,
                model: None,
                runner: None,
                condition: None,
                append_system_prompt: None,
                system_prompt: None,
                resume: false,
                on_error: None,
                before: vec![],
                then: vec![],
                output_schema: None,
            }],
            source: None,
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
            named_pipelines: HashMap::new(),
        }
    }
}

/// Controls whether a step executes (SPEC §12).
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// Step always executes (same as omitting `condition:`).
    Always,
    /// Step is unconditionally skipped.
    Never,
    /// Expression condition evaluated at runtime against session state (SPEC §12.2).
    /// The string may contain `{{ variable }}` template syntax which is resolved
    /// before evaluating the expression operator.
    Expression(ConditionExpr),
}

/// A parsed condition expression (SPEC §12.2).
///
/// The `lhs` is a template string (e.g. `"{{ step.test.exit_code }}"`) that is
/// resolved at runtime. The `rhs` is a literal value to compare against.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionExpr {
    /// Left-hand side — a template string resolved at evaluation time.
    pub lhs: String,
    /// Comparison operator.
    pub op: ConditionOp,
    /// Right-hand side — a literal value.
    pub rhs: String,
}

/// Comparison operators for condition expressions (SPEC §12.2).
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionOp {
    /// `==` — string equality after trimming.
    Eq,
    /// `!=` — string inequality after trimming.
    Ne,
    /// `contains` — left-hand side contains the right-hand side (case-insensitive).
    Contains,
    /// `starts_with` — left-hand side starts with the right-hand side (case-insensitive).
    StartsWith,
    /// `ends_with` — left-hand side ends with the right-hand side (case-insensitive).
    EndsWith,
}

/// Error handling strategy for a step (SPEC §16).
///
/// Determines what happens when a step fails with an execution error
/// (runner crash, timeout, network failure, etc.). Does NOT fire for
/// non-zero shell exit codes — those are results, not errors.
#[derive(Debug, Clone, PartialEq)]
pub enum OnError {
    /// Log the error and proceed to the next step.
    Continue,
    /// Retry the failed step up to `max_retries` times, then abort.
    Retry { max_retries: u32 },
    /// Stop execution immediately (default behaviour).
    AbortPipeline,
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
    /// Optional condition controlling whether this step executes (SPEC §12).
    /// `None` means always execute (same as `Some(Condition::Always)`).
    pub condition: Option<Condition>,
    /// Ordered list of system context additions appended to the system prompt at step runtime (SPEC §5.9).
    pub append_system_prompt: Option<Vec<SystemPromptEntry>>,
    /// Optional system prompt override for this step (SPEC §5.9).
    /// When set, replaces the runner's default system prompt entirely.
    /// May be an inline string or a file path (resolved via `resolve_prompt_file`).
    pub system_prompt: Option<String>,
    /// Whether this step resumes the previous runner session (SPEC §15.4).
    /// `false` (default) starts a fresh session for each step.
    pub resume: bool,
    /// Error handling strategy for this step (SPEC §16).
    /// `None` means abort (default behaviour — same as `Some(OnError::AbortPipeline)`).
    pub on_error: Option<OnError>,
    /// Private pre-processing steps that run before this step fires (SPEC §5.10).
    /// Steps in this chain are not visible to the hook system and not independently referenceable.
    pub before: Vec<Step>,
    /// Private post-processing steps that run after this step completes (SPEC §5.7).
    /// Steps in this chain are not visible to the hook system and not independently referenceable.
    pub then: Vec<Step>,
    /// Optional JSON Schema for validating this step's output (SPEC §26).
    /// When set, the runtime validates the step's response as JSON against this schema
    /// after execution. A validation failure escalates via `on_error`.
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone)]
pub struct ToolPolicy {
    /// When `true`, disables all tools for this step. Overrides `allow` and `deny`. SPEC §5.8.
    pub disabled: bool,
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
    /// A named skill invocation (SPEC §6). The skill name may reference a built-in
    /// module (e.g. `ail/code_review`) or a project-local skill. The skill registry
    /// resolves the name to a prompt template at execution time.
    Skill {
        name: String,
    },
    /// Path to a sub-pipeline YAML file. May contain `{{ variable }}` syntax;
    /// the path is template-resolved at execution time (SPEC §11).
    /// `prompt` overrides the child session's invocation prompt when set;
    /// defaults to parent's last response (SPEC §9).
    SubPipeline {
        path: String,
        prompt: Option<String>,
    },
    /// Reference to a named pipeline defined in the `pipelines:` section (SPEC §10).
    /// The `name` is resolved against `Pipeline.named_pipelines` at execution time.
    /// `prompt` overrides the child session's invocation prompt when set;
    /// defaults to parent's last response.
    NamedPipeline {
        name: String,
        prompt: Option<String>,
    },
    Action(ActionKind),
    // Note: Named pipeline references from `pipeline:` step bodies that match a key
    // in the `pipelines:` map are resolved to `NamedPipeline` during a second pass
    // in validation. If the value does not match a named pipeline key, it remains
    // as `SubPipeline` (file-based sub-pipeline reference).
    Context(ContextSource),
    /// Bounded repeat-until loop (SPEC §27). Inner steps execute repeatedly until
    /// `exit_when` evaluates to true or `max_iterations` is reached.
    DoWhile {
        /// Maximum number of iterations (≥ 1, required, no default).
        max_iterations: u64,
        /// Condition expression evaluated after each complete iteration.
        /// Uses the same expression syntax as `condition:` (SPEC §12.2).
        exit_when: ConditionExpr,
        /// Inner steps executed each iteration.
        steps: Vec<Step>,
    },
}

#[derive(Debug, Clone)]
pub enum ContextSource {
    Shell(String),
}

#[derive(Debug, Clone)]
pub enum ActionKind {
    PauseForHuman,
    /// HITL modify gate (SPEC §13.2): presents step output to the human, who can edit it.
    /// The modified text is stored in the turn log and available as `{{ step.<id>.modified }}`.
    /// `headless_behavior` controls what happens when this action fires in headless/`--once` mode.
    ModifyOutput {
        headless_behavior: HitlHeadlessBehavior,
        /// Optional default value used when `headless_behavior` is `UseDefault`.
        default_value: Option<String>,
    },
}

/// Behavior for HITL gates in headless/`--once` mode (SPEC §13.2).
#[derive(Debug, Clone, PartialEq)]
pub enum HitlHeadlessBehavior {
    /// Skip the gate and continue the pipeline with the unmodified output (default).
    Skip,
    /// Abort the pipeline with a PIPELINE_ABORTED error.
    Abort,
    /// Use a configured default value as the modified output.
    UseDefault,
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
    /// `prompt` overrides the child session's invocation prompt when set;
    /// defaults to parent's last response.
    Pipeline {
        path: String,
        prompt: Option<String>,
    },
}
