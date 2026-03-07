use std::path::PathBuf;

#[derive(Debug)]
pub struct Pipeline {
    pub steps: Vec<Step>,
    pub source: Option<PathBuf>,
}

impl Pipeline {
    /// Zero-step passthrough pipeline — the safe default when no `.ail.yaml` is found (SPEC §3.1).
    pub fn passthrough() -> Self {
        Pipeline {
            steps: vec![],
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
