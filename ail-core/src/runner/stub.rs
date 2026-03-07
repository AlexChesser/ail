use super::{RunResult, Runner};
use crate::error::AilError;

/// A deterministic runner for use in unit tests.
/// Returns a fixed response without invoking any external process.
pub struct StubRunner {
    pub response: String,
    pub cost_usd: Option<f64>,
}

impl StubRunner {
    pub fn new(response: impl Into<String>) -> Self {
        StubRunner {
            response: response.into(),
            cost_usd: Some(0.0),
        }
    }
}

impl Runner for StubRunner {
    fn invoke(&self, _prompt: &str) -> Result<RunResult, AilError> {
        Ok(RunResult {
            response: self.response.clone(),
            cost_usd: self.cost_usd,
            session_id: Some("stub-session-id".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_runner_returns_configured_response() {
        let runner = StubRunner::new("stub response");
        let result = runner.invoke("any prompt").unwrap();
        assert_eq!(result.response, "stub response");
    }

    #[test]
    fn stub_runner_ignores_prompt_content() {
        let runner = StubRunner::new("fixed");
        let r1 = runner.invoke("prompt one").unwrap();
        let r2 = runner.invoke("prompt two").unwrap();
        assert_eq!(r1.response, r2.response);
    }
}
