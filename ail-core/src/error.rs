/// Hint returned by [`AilError::recovery_strategy`] telling callers how to react.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStrategy {
    /// Fatal — stop the pipeline immediately.
    Abort,
    /// Skip this step and continue the pipeline.
    Skip,
    /// Transient failure — retry up to `max_attempts` times.
    Retry { max_attempts: u32 },
    /// Transient failure — retry with exponential backoff.
    RetryWithBackoff { max_attempts: u32, base_ms: u64 },
    /// Unrecoverable without user input.
    HumanIntervention,
}

#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub pipeline_run_id: Option<String>,
    pub step_id: Option<String>,
    pub source: Option<String>,
}

impl ErrorContext {
    /// Construct a step-scoped context with `pipeline_run_id` and `step_id` set.
    pub fn for_step(run_id: impl Into<String>, step_id: impl Into<String>) -> Self {
        Self {
            pipeline_run_id: Some(run_id.into()),
            step_id: Some(step_id.into()),
            source: None,
        }
    }
}

/// RFC 9457-inspired error type used throughout ail-core.
///
/// The `error_type` field is a stable namespaced identifier consumed by downstream
/// tooling (VS Code extension, NDJSON output). It must not change across releases.
///
/// Use the constructor helpers (`AilError::config_validation(...)` etc.) to build
/// errors concisely rather than spelling out all four fields every time.
#[derive(Debug, thiserror::Error)]
#[error("[{error_type}] {title}: {detail}")]
pub struct AilError {
    pub error_type: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub context: Option<ErrorContext>,
}

impl AilError {
    // ── Constructors ──────────────────────────────────────────────────────────

    pub fn config_not_found(title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::CONFIG_FILE_NOT_FOUND,
            title,
            detail: detail.into(),
            context: None,
        }
    }

    pub fn config_invalid_yaml(title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::CONFIG_INVALID_YAML,
            title,
            detail: detail.into(),
            context: None,
        }
    }

    pub fn config_validation(title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::CONFIG_VALIDATION_FAILED,
            title,
            detail: detail.into(),
            context: None,
        }
    }

    pub fn template_unresolved(title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::TEMPLATE_UNRESOLVED,
            title,
            detail: detail.into(),
            context: None,
        }
    }

    pub fn runner_failed(title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title,
            detail: detail.into(),
            context: None,
        }
    }

    pub fn runner_cancelled(detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::RUNNER_CANCELLED,
            title: "Runner cancelled",
            detail: detail.into(),
            context: None,
        }
    }

    pub fn runner_not_found(detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::RUNNER_NOT_FOUND,
            title: "Unknown runner",
            detail: detail.into(),
            context: None,
        }
    }

    pub fn pipeline_aborted(title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error_type: error_types::PIPELINE_ABORTED,
            title,
            detail: detail.into(),
            context: None,
        }
    }

    // ── Context attachment ────────────────────────────────────────────────────

    /// Attach step-scoped context to this error, returning `self`.
    pub fn with_step_context(mut self, run_id: &str, step_id: &str) -> Self {
        self.context = Some(ErrorContext::for_step(run_id, step_id));
        self
    }

    // ── Recovery hint ─────────────────────────────────────────────────────────

    /// Return a [`RecoveryStrategy`] describing how the caller should react.
    ///
    /// Callers are not required to act on this hint — it is advisory. The executor
    /// does not currently act on recovery strategies; they are intended for future
    /// use in retry loops and step-skip logic.
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        match self.error_type {
            error_types::CONFIG_INVALID_YAML => RecoveryStrategy::Abort,
            error_types::CONFIG_FILE_NOT_FOUND => RecoveryStrategy::Abort,
            error_types::CONFIG_VALIDATION_FAILED => RecoveryStrategy::Abort,
            error_types::TEMPLATE_UNRESOLVED => RecoveryStrategy::Abort,
            error_types::RUNNER_INVOCATION_FAILED => RecoveryStrategy::Retry { max_attempts: 2 },
            error_types::RUNNER_CANCELLED => RecoveryStrategy::Abort,
            error_types::RUNNER_NOT_FOUND => RecoveryStrategy::Abort,
            error_types::PIPELINE_ABORTED => RecoveryStrategy::Abort,
            _ => RecoveryStrategy::Abort,
        }
    }
}

pub mod error_types {
    pub const CONFIG_INVALID_YAML: &str = "ail:config/invalid-yaml";
    pub const CONFIG_FILE_NOT_FOUND: &str = "ail:config/file-not-found";
    pub const CONFIG_VALIDATION_FAILED: &str = "ail:config/validation-failed";
    pub const TEMPLATE_UNRESOLVED: &str = "ail:template/unresolved-variable";
    pub const RUNNER_INVOCATION_FAILED: &str = "ail:runner/invocation-failed";
    pub const RUNNER_CANCELLED: &str = "ail:runner/cancelled";
    pub const RUNNER_NOT_FOUND: &str = "ail:runner/not-found";
    pub const PIPELINE_ABORTED: &str = "ail:pipeline/aborted";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_err() -> AilError {
        AilError {
            error_type: error_types::CONFIG_INVALID_YAML,
            title: "Invalid YAML",
            detail: "unexpected token at line 3".to_string(),
            context: None,
        }
    }

    #[test]
    fn display_contains_error_type() {
        let err = make_err();
        assert!(err.to_string().contains(error_types::CONFIG_INVALID_YAML));
    }

    #[test]
    fn display_contains_detail() {
        let err = make_err();
        assert!(err.to_string().contains("unexpected token at line 3"));
    }

    #[test]
    fn test_with_step_context() {
        let err = AilError {
            error_type: "test",
            title: "Test",
            detail: "test".to_string(),
            context: None,
        };
        let err = err.with_step_context("run-1", "step-1");
        let ctx = err.context.unwrap();
        assert_eq!(ctx.pipeline_run_id, Some("run-1".to_string()));
        assert_eq!(ctx.step_id, Some("step-1".to_string()));
        assert!(ctx.source.is_none());
    }

    #[test]
    fn none_context_does_not_affect_display() {
        let err_with = AilError {
            error_type: error_types::CONFIG_INVALID_YAML,
            title: "Invalid YAML",
            detail: "some detail".to_string(),
            context: Some(ErrorContext {
                pipeline_run_id: Some("run-1".to_string()),
                step_id: None,
                source: None,
            }),
        };
        let err_without = AilError {
            error_type: error_types::CONFIG_INVALID_YAML,
            title: "Invalid YAML",
            detail: "some detail".to_string(),
            context: None,
        };
        assert_eq!(err_with.to_string(), err_without.to_string());
    }

    #[test]
    fn constructors_set_correct_error_type() {
        assert_eq!(
            AilError::config_not_found("t", "d").error_type,
            error_types::CONFIG_FILE_NOT_FOUND
        );
        assert_eq!(
            AilError::config_invalid_yaml("t", "d").error_type,
            error_types::CONFIG_INVALID_YAML
        );
        assert_eq!(
            AilError::config_validation("t", "d").error_type,
            error_types::CONFIG_VALIDATION_FAILED
        );
        assert_eq!(
            AilError::template_unresolved("t", "d").error_type,
            error_types::TEMPLATE_UNRESOLVED
        );
        assert_eq!(
            AilError::runner_failed("t", "d").error_type,
            error_types::RUNNER_INVOCATION_FAILED
        );
        assert_eq!(
            AilError::runner_not_found("d").error_type,
            error_types::RUNNER_NOT_FOUND
        );
        assert_eq!(
            AilError::pipeline_aborted("t", "d").error_type,
            error_types::PIPELINE_ABORTED
        );
        assert_eq!(
            AilError::runner_cancelled("d").error_type,
            error_types::RUNNER_CANCELLED
        );
    }

    #[test]
    fn recovery_strategy_runner_failed_is_retry() {
        let err = AilError::runner_failed("t", "d");
        assert_eq!(
            err.recovery_strategy(),
            RecoveryStrategy::Retry { max_attempts: 2 }
        );
    }

    #[test]
    fn recovery_strategy_config_errors_abort() {
        assert_eq!(
            AilError::config_validation("t", "d").recovery_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            AilError::config_not_found("t", "d").recovery_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            AilError::config_invalid_yaml("t", "d").recovery_strategy(),
            RecoveryStrategy::Abort
        );
    }
}
