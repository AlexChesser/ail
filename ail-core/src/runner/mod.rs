#![allow(clippy::result_large_err)]

pub mod claude;
pub mod stub;

use crate::error::AilError;

pub struct RunResult {
    pub response: String,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
}

/// Options passed to a runner invocation. Extensible without changing the trait signature.
#[derive(Default)]
pub struct InvokeOptions {
    /// Resumes an existing conversation by session ID (passed as `--resume <id>`).
    pub resume_session_id: Option<String>,
    /// Tools pre-approved for this step — passed as `--allowedTools` (SPEC §5.6).
    pub allowed_tools: Vec<String>,
    /// Tools pre-denied for this step — passed as `--disallowedTools` (SPEC §5.6).
    pub denied_tools: Vec<String>,
}

pub trait Runner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError>;
}
