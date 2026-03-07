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
    fn invoke(&self, prompt: &str) -> Result<RunResult, AilError>;
}
