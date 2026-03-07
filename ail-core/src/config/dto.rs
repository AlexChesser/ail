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
    pub tools: Option<ToolsDto>,
}

#[derive(Deserialize)]
pub struct ToolsDto {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}
