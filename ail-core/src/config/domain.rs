use std::path::PathBuf;

#[derive(Debug)]
pub struct Pipeline {
    pub steps: Vec<Step>,
    pub source: Option<PathBuf>,
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
                tools: None,
                on_result: None,
            }],
            source: None,
        }
    }
}

#[derive(Debug)]
pub struct Step {
    pub id: StepId,
    pub body: StepBody,
    /// Pre-approved and pre-denied tools for this step (SPEC §5.8).
    /// Passed as `--allowedTools` / `--disallowedTools` to the runner.
    pub tools: Option<ToolPolicy>,
    /// Declarative branching after step completion (SPEC §5.4).
    pub on_result: Option<Vec<ResultBranch>>,
}

#[derive(Debug, Default)]
pub struct ToolPolicy {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug)]
pub struct StepId(pub String);

impl StepId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub enum StepBody {
    Prompt(String),
    Skill(PathBuf),
    SubPipeline(PathBuf),
    Action(ActionKind),
    Context(ContextSource),
}

#[derive(Debug)]
pub enum ContextSource {
    Shell(String),
}

#[derive(Debug)]
pub enum ActionKind {
    PauseForHuman,
}

/// One branch in an `on_result` multi-branch array (SPEC §5.4).
#[derive(Debug)]
pub struct ResultBranch {
    pub matcher: ResultMatcher,
    pub action: ResultAction,
}

#[derive(Debug)]
pub enum ResultMatcher {
    Contains(String),
    ExitCode(ExitCodeMatch),
    Always,
}

/// Matches process exit codes in `on_result` branches.
/// `Any` matches any non-zero exit code. Does not match 0.
#[derive(Debug)]
pub enum ExitCodeMatch {
    Exact(i32),
    Any,
}

#[derive(Debug)]
pub enum ResultAction {
    Continue,
    Break,
    AbortPipeline,
    PauseForHuman,
}
