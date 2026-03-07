#![allow(clippy::result_large_err)]

pub mod claude;
pub mod stub;

use crate::error::AilError;

pub struct RunResult {
    pub response: String,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
}

pub trait Runner {
    /// Invoke the runner with `prompt`.
    ///
    /// `resume_session_id` — if provided, passes `--resume <id>` to the claude
    /// CLI so the invocation continues an existing conversation. This is how
    /// pipeline steps get access to the conversation history from the initial
    /// `--once` invocation.
    fn invoke(&self, prompt: &str, resume_session_id: Option<&str>) -> Result<RunResult, AilError>;
}
