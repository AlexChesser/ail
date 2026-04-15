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

/// Maximum number of `action: reload_self` invocations per pipeline run (SPEC §21).
/// Prevents infinite self-rewrite loops — the LLM can edit `.ail.yaml` and reload,
/// but only a bounded number of times within a single `ail --once` invocation.
pub const MAX_RELOADS_PER_RUN: usize = 16;

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
    /// Provider-attached sampling defaults (SPEC §30.2). Applied with higher
    /// precedence than pipeline-wide `sampling_defaults` and lower than per-step
    /// `step.sampling`. Field-level merge within the sampling block.
    pub sampling: Option<SamplingConfig>,
}

impl ProviderConfig {
    /// Merge another `ProviderConfig` on top of `self`, with `other` taking precedence.
    /// Fields present in `other` override fields in `self`; absent fields fall through.
    /// Sampling blocks are field-merged recursively, not replaced.
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
            sampling: match (self.sampling, other.sampling) {
                (Some(a), Some(b)) => Some(a.merge(b)),
                (a, b) => b.or(a),
            },
        }
    }
}

/// Sampling parameter configuration (SPEC §30). Reusable block shared across
/// three scopes: pipeline defaults, provider-attached, and per-step.
///
/// All fields are `Option<T>` so scopes can merge field-by-field — absent
/// fields fall through from lower-precedence scopes.
#[derive(Debug, Clone, Default)]
pub struct SamplingConfig {
    /// Sampling temperature. Range [0.0, 2.0]. Lower = deterministic, higher = diverse.
    pub temperature: Option<f64>,
    /// Nucleus sampling cutoff. Range [0.0, 1.0]. Keeps tokens in the top `p` probability mass.
    pub top_p: Option<f64>,
    /// Top-K sampling cutoff. Keeps only the top `k` most-likely tokens. Not universally supported.
    pub top_k: Option<u64>,
    /// Maximum number of tokens in the response.
    pub max_tokens: Option<u64>,
    /// Stop generation when any of these strings is produced. Replace (not append) semantics
    /// across scopes — SPEC §30.3.1.
    pub stop_sequences: Option<Vec<String>>,
    /// Reasoning / extended-thinking intensity as a fraction in [0.0, 1.0]. `0.0` = off;
    /// `1.0` = maximum. Each runner quantizes to whatever granularity it supports.
    pub thinking: Option<f64>,
}

impl SamplingConfig {
    /// Merge `other` on top of `self`. `other` wins per-field; absent fields
    /// fall through from `self`. `stop_sequences` follows replace semantics
    /// (SPEC §30.3.1) — if `other.stop_sequences` is `Some(_)`, it replaces
    /// `self.stop_sequences` entirely rather than appending.
    pub fn merge(self, other: SamplingConfig) -> SamplingConfig {
        SamplingConfig {
            temperature: other.temperature.or(self.temperature),
            top_p: other.top_p.or(self.top_p),
            top_k: other.top_k.or(self.top_k),
            max_tokens: other.max_tokens.or(self.max_tokens),
            stop_sequences: other.stop_sequences.or(self.stop_sequences),
            thinking: other.thinking.or(self.thinking),
        }
    }

    /// `true` when no field is set — used by the resolver to return `None`
    /// rather than an empty `SamplingConfig`, so runners can cheaply skip
    /// the sampling path when the author set nothing at any scope.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.top_k.is_none()
            && self.max_tokens.is_none()
            && self.stop_sequences.is_none()
            && self.thinking.is_none()
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
    /// Pipeline-wide concurrency cap for async steps (SPEC §29.10). `None` means unlimited.
    pub max_concurrency: Option<u64>,
    /// Pipeline-wide sampling defaults (SPEC §30.2). Orthogonal to provider —
    /// applied as the lowest-precedence scope in the three-scope merge chain
    /// (pipeline defaults → provider-attached → per-step).
    pub sampling_defaults: Option<SamplingConfig>,
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
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}

/// Controls whether a step executes (SPEC §12).
///
/// `PartialEq` is intentionally not derived: `Regex` variants carry a
/// compiled [`regex::Regex`], which does not implement `PartialEq`. Compare
/// via pattern-match (`matches!`) or by inspecting the contained source.
#[derive(Debug, Clone)]
pub enum Condition {
    /// Step always executes (same as omitting `condition:`).
    Always,
    /// Step is unconditionally skipped.
    Never,
    /// Comparison expression evaluated at runtime against session state (SPEC §12.2).
    /// The string may contain `{{ variable }}` template syntax which is resolved
    /// before evaluating the expression operator.
    Expression(ConditionExpr),
    /// Regex-match expression evaluated at runtime (SPEC §12.2 `matches` operator,
    /// regex semantics from §12.3). The pattern is compiled at parse time; an
    /// invalid regex fails pipeline load, not evaluation.
    Regex(RegexCondition),
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

/// A parsed regex-match condition (SPEC §12.2 `matches` / §12.3).
///
/// The `lhs` is a template string resolved at evaluation time; the compiled
/// regex is applied to the resolved string. `source` preserves the original
/// `/PATTERN/FLAGS` literal for error messages and materialize output.
#[derive(Debug, Clone)]
pub struct RegexCondition {
    /// Left-hand side — a template string resolved at evaluation time.
    pub lhs: String,
    /// Compiled regex, built at parse time per §12.3.
    pub regex: regex::Regex,
    /// Original source literal, e.g. `/warn|error/i`. Preserved for diagnostics.
    pub source: String,
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
    /// Whether this step runs asynchronously / non-blocking (SPEC §29.1).
    /// When `true`, the pipeline cursor advances immediately after launching.
    pub async_step: bool,
    /// Step IDs this step must wait for before executing (SPEC §29.1).
    /// Empty for most steps. Non-empty implies a synchronization barrier.
    pub depends_on: Vec<StepId>,
    /// Error handling strategy for this step (SPEC §16).
    /// `None` means abort (default behaviour — same as `Some(OnError::AbortPipeline)`).
    pub on_error: Option<OnError>,
    /// Private pre-processing steps that run before this step fires (SPEC §5.10).
    /// Steps in this chain are not visible to the hook system and not independently referenceable.
    pub before: Vec<Step>,
    /// Private post-processing steps that run after this step completes (SPEC §5.7).
    /// Steps in this chain are not visible to the hook system and not independently referenceable.
    pub then: Vec<Step>,
    /// Optional JSON Schema for validating this step's output (SPEC §26.1).
    /// When set, the runtime validates the step's response as JSON against this schema
    /// after execution. A validation failure escalates via `on_error`.
    pub output_schema: Option<serde_json::Value>,
    /// Optional JSON Schema for validating the preceding step's output (SPEC §26.2).
    /// When set, the runtime validates the preceding step's response against this schema
    /// before this step executes. A validation failure escalates via `on_error`.
    pub input_schema: Option<serde_json::Value>,
    /// Per-step sampling parameter override (SPEC §30). Highest-precedence scope in the
    /// three-scope merge chain. Field-level merge with pipeline and provider-attached
    /// defaults; `stop_sequences` follows replace semantics (SPEC §30.3.1).
    pub sampling: Option<SamplingConfig>,
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
    /// Collection iteration (SPEC §28). Runs inner steps once per item in a
    /// validated array from a prior step's `output_schema: type: array`.
    ForEach {
        /// Template expression resolving to a validated JSON array.
        over: String,
        /// Local name for the current item (default: `item`).
        as_name: String,
        /// Optional hard cap on items processed.
        max_items: Option<u64>,
        /// What happens when the array exceeds `max_items`.
        on_max_items: OnMaxItems,
        /// Inner steps executed once per item.
        steps: Vec<Step>,
    },
}

/// Behavior when a `for_each:` array exceeds `max_items` (SPEC §28.2).
#[derive(Debug, Clone, PartialEq)]
pub enum OnMaxItems {
    /// Silently skip excess items (default).
    Continue,
    /// Treat excess items as a fatal error.
    AbortPipeline,
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
    /// Synchronization barrier that waits for all `depends_on` steps to complete,
    /// merges their outputs, and makes the result available to downstream steps (SPEC §29.3).
    Join {
        /// Error handling mode — controls behavior when a dependency fails (SPEC §29.7).
        on_error_mode: JoinErrorMode,
    },
    /// Hot-reload the active pipeline from its source file on disk (SPEC §21).
    /// Re-parses `session.pipeline.source`, validates, swaps `session.pipeline`
    /// in place, and re-anchors the top-level sequential loop by matching the
    /// reload step's own id in the new step list.
    ReloadSelf,
}

/// Error handling mode for `action: join` steps (SPEC §29.7).
#[derive(Debug, Clone, PartialEq)]
pub enum JoinErrorMode {
    /// Default — first dependency failure cancels all other in-flight branches.
    FailFast,
    /// All branches run to completion regardless of individual failures.
    /// Failed branches contribute error envelopes to the merged output.
    WaitForAll,
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
    /// Exact equality match against a named field in validated JSON input (SPEC §26.4).
    /// Requires the step to declare an `input_schema` containing the referenced field.
    Field {
        name: String,
        equals: serde_json::Value,
    },
    /// Arbitrary §12.2 expression matcher (SPEC §5.4 `expression:`). Accepts the full
    /// `Condition` union because the condition parser returns `Expression` for
    /// comparison operators and `Regex` for the `matches` operator.
    ///
    /// The `source` field preserves the original expression string for diagnostics
    /// and materialize output. `Condition::Always`/`Never` do not occur here —
    /// only `Expression` and `Regex` variants are produced by the expression
    /// parser used for this matcher.
    Expression {
        source: String,
        condition: Condition,
    },
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
