use serde::Deserialize;

#[derive(Deserialize)]
pub struct PipelineFileDto {
    pub version: Option<String>,
    pub pipeline: Option<Vec<StepDto>>,
}

#[derive(Deserialize)]
pub struct StepDto {
    pub id: Option<String>,
    pub prompt: Option<String>,
    pub skill: Option<String>,
    pub pipeline: Option<String>,
    pub action: Option<String>,
    pub context: Option<ContextDto>,
    pub tools: Option<ToolsDto>,
    pub on_result: Option<Vec<OnResultBranchDto>>,
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
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}
