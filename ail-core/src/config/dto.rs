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
}
