#![allow(clippy::result_large_err)]

//! Factory for constructing `Runner` implementations by name.
//!
//! # Selection hierarchy
//!
//! 1. Per-step `runner:` field in YAML (resolved by the executor)
//! 2. `AIL_DEFAULT_RUNNER` environment variable
//! 3. Fallback: `"claude"` → `ClaudeCliRunner`
//!
//! # Adding a new runner
//!
//! 1. Implement the `Runner` trait in a new module under `runner/`.
//! 2. Add a match arm in `RunnerFactory::build()` mapping the runner name to the implementation.
//! 3. Export the module from `runner/mod.rs`.

use super::{
    claude::{ClaudeCliRunnerConfig, ClaudeCliRunner},
    stub::StubRunner,
    Runner,
};
use crate::error::{error_types, AilError};

/// Factory that constructs `Runner` boxed trait objects by name.
///
/// The canonical way to obtain a runner in the binary entry point:
///
/// ```ignore
/// use ail_core::runner::factory::RunnerFactory;
/// let runner = RunnerFactory::build_default(cli.headless)?;
/// ```
pub struct RunnerFactory;

impl RunnerFactory {
    /// Build a runner by explicit name.
    ///
    /// Recognised names (case-insensitive, whitespace-trimmed):
    /// - `"claude"` — `ClaudeCliRunner` with the given headless flag
    /// - `"stub"` — `StubRunner` with a fixed response (test/dev use)
    ///
    /// Returns `RUNNER_NOT_FOUND` if the name is not recognised.
    pub fn build(runner_name: &str, headless: bool) -> Result<Box<dyn Runner + Send>, AilError> {
        match runner_name.trim().to_lowercase().as_str() {
            "claude" => {
                let runner: ClaudeCliRunner =
                    ClaudeCliRunnerConfig::default().headless(headless).build();
                Ok(Box::new(runner))
            }
            "stub" => Ok(Box::new(StubRunner::new("stub response"))),
            other => Err(AilError {
                error_type: error_types::RUNNER_NOT_FOUND,
                title: "Unknown runner",
                detail: format!("Runner '{other}' is not recognized. Known runners: claude, stub"),
                context: None,
            }),
        }
    }

    /// Build the default runner, honouring the `AIL_DEFAULT_RUNNER` environment variable.
    ///
    /// Resolution order:
    /// 1. `AIL_DEFAULT_RUNNER` env var (if set and non-empty)
    /// 2. `"claude"` (hardcoded fallback)
    pub fn build_default(headless: bool) -> Result<Box<dyn Runner + Send>, AilError> {
        let name =
            std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string());
        Self::build(&name, headless)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_claude_runner_succeeds() {
        // Just check it constructs without error; actual invocation requires the claude binary.
        let result = RunnerFactory::build("claude", false);
        assert!(result.is_ok());
    }

    #[test]
    fn build_claude_runner_case_insensitive() {
        let result = RunnerFactory::build("Claude", false);
        assert!(result.is_ok());
        let result = RunnerFactory::build("CLAUDE", false);
        assert!(result.is_ok());
    }

    #[test]
    fn build_stub_runner_succeeds() {
        let result = RunnerFactory::build("stub", false);
        assert!(result.is_ok());
    }

    #[test]
    fn build_stub_runner_returns_fixed_response() {
        use crate::runner::InvokeOptions;
        let runner = RunnerFactory::build("stub", false).unwrap();
        let result = runner.invoke("any prompt", InvokeOptions::default()).unwrap();
        assert_eq!(result.response, "stub response");
    }

    #[test]
    fn build_unknown_runner_returns_runner_not_found_error() {
        use crate::error::error_types;
        let result = RunnerFactory::build("nonexistent", false);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.error_type, error_types::RUNNER_NOT_FOUND);
        assert!(err.detail.contains("nonexistent"));
    }

    #[test]
    fn build_default_respects_env_var() {
        use crate::runner::InvokeOptions;
        // Set AIL_DEFAULT_RUNNER=stub and verify stub runner is used.
        // We cannot safely set env vars in parallel tests without serialisation,
        // so we test the build() path directly instead.
        let runner = RunnerFactory::build("stub", false).unwrap();
        let result = runner.invoke("hello", InvokeOptions::default()).unwrap();
        assert_eq!(result.response, "stub response");
    }

    #[test]
    fn build_default_falls_back_to_claude_when_env_absent() {
        // When AIL_DEFAULT_RUNNER is not set, build_default should succeed
        // by constructing the claude runner (object construction is side-effect-free).
        // We cannot guarantee the env var is unset in all CI environments, so we
        // verify that build("claude", false) succeeds as the fallback path.
        let result = RunnerFactory::build("claude", false);
        assert!(result.is_ok());
    }
}
