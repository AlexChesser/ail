use super::{InvokeOptions, RunResult, Runner, ToolPermissionPolicy};
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
    fn invoke(&self, _prompt: &str, _options: InvokeOptions) -> Result<RunResult, AilError> {
        Ok(RunResult {
            response: self.response.clone(),
            cost_usd: self.cost_usd,
            session_id: Some("stub-session-id".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            thinking: None,
            model: None,
            tool_events: vec![],
        })
    }
}

/// A stub runner that counts invocations. Used to verify context steps bypass the runner.
pub struct CountingStubRunner {
    response: String,
    count: std::sync::atomic::AtomicU32,
}

impl CountingStubRunner {
    pub fn new(response: impl Into<String>) -> Self {
        CountingStubRunner {
            response: response.into(),
            count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    pub fn invocation_count(&self) -> u32 {
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Runner for CountingStubRunner {
    fn invoke(&self, _prompt: &str, _options: InvokeOptions) -> Result<RunResult, AilError> {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(RunResult {
            response: self.response.clone(),
            cost_usd: Some(0.0),
            session_id: Some("stub-session-id".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            thinking: None,
            model: None,
            tool_events: vec![],
        })
    }
}

/// A stub runner that echoes the prompt back as the response.
/// Useful for testing that a specific prompt value reaches the runner.
pub struct EchoStubRunner;

impl EchoStubRunner {
    pub fn new() -> Self {
        EchoStubRunner
    }
}

impl Default for EchoStubRunner {
    fn default() -> Self {
        EchoStubRunner
    }
}

impl Runner for EchoStubRunner {
    fn invoke(&self, prompt: &str, _options: InvokeOptions) -> Result<RunResult, AilError> {
        Ok(RunResult {
            response: prompt.to_string(),
            cost_usd: Some(0.0),
            session_id: Some("echo-stub-session-id".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            thinking: None,
            model: None,
            tool_events: vec![],
        })
    }
}

/// A recorded invocation call — captures the `tool_policy` for assertion in tests.
pub struct RecordedCall {
    pub prompt: String,
    pub tool_policy: ToolPermissionPolicy,
}

/// A stub runner that records each invocation for inspection in tests.
/// Used to verify the tool policy passed to the runner.
pub struct RecordingStubRunner {
    response: String,
    calls: std::sync::Mutex<Vec<RecordedCall>>,
}

impl RecordingStubRunner {
    pub fn new(response: impl Into<String>) -> Self {
        RecordingStubRunner {
            response: response.into(),
            calls: std::sync::Mutex::new(vec![]),
        }
    }

    /// Returns the list of recorded invocation calls.
    pub fn calls(&self) -> std::sync::MutexGuard<'_, Vec<RecordedCall>> {
        self.calls.lock().expect("mutex not poisoned")
    }
}

impl Runner for RecordingStubRunner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        self.calls
            .lock()
            .expect("mutex not poisoned")
            .push(RecordedCall {
                prompt: prompt.to_string(),
                tool_policy: options.tool_policy,
            });
        Ok(RunResult {
            response: self.response.clone(),
            cost_usd: Some(0.0),
            session_id: Some("recording-stub-session-id".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            thinking: None,
            model: None,
            tool_events: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_runner_returns_configured_response() {
        let runner = StubRunner::new("stub response");
        let result = runner
            .invoke("any prompt", InvokeOptions::default())
            .unwrap();
        assert_eq!(result.response, "stub response");
    }

    #[test]
    fn stub_runner_ignores_prompt_content() {
        let runner = StubRunner::new("fixed");
        let r1 = runner
            .invoke("prompt one", InvokeOptions::default())
            .unwrap();
        let r2 = runner
            .invoke("prompt two", InvokeOptions::default())
            .unwrap();
        assert_eq!(r1.response, r2.response);
    }
}
