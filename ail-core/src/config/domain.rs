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
            }],
            source: None,
        }
    }
}

#[derive(Debug)]
pub struct Step {
    pub id: StepId,
    pub body: StepBody,
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
}

#[derive(Debug)]
pub enum ActionKind {
    PauseForHuman,
}
