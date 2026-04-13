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
/// Each variant encodes a stable `error_type` string consumed by downstream tooling
/// (VS Code extension, NDJSON output). The `error_type()` method returns that string.
/// Use `detail()` / `into_detail()` to access the human-readable detail message.
///
/// Prefer the variant constructors over `thiserror`'s generated field syntax where
/// possible to keep construction sites concise.
#[derive(Debug, thiserror::Error)]
pub enum AilError {
    #[error("[ail:config/invalid-yaml] {detail}")]
    ConfigInvalidYaml {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:config/file-not-found] {detail}")]
    ConfigFileNotFound {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:config/validation-failed] {detail}")]
    ConfigValidationFailed {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:template/unresolved-variable] {detail}")]
    TemplateUnresolved {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:runner/invocation-failed] {detail}")]
    RunnerInvocationFailed {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:runner/cancelled] {detail}")]
    RunnerCancelled {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:runner/not-found] {detail}")]
    RunnerNotFound {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:pipeline/aborted] {detail}")]
    PipelineAborted {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:storage/query-failed] {detail}")]
    StorageQueryFailed {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:storage/run-not-found] {detail}")]
    RunNotFound {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:storage/delete-failed] {detail}")]
    StorageDeleteFailed {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:plugin/manifest-invalid] {detail}")]
    PluginManifestInvalid {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:plugin/spawn-failed] {detail}")]
    PluginSpawnFailed {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:plugin/protocol-error] {detail}")]
    PluginProtocolError {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:plugin/timeout] {detail}")]
    PluginTimeout {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:condition/invalid] {detail}")]
    ConditionInvalid {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:pipeline/circular-reference] {detail}")]
    PipelineCircularReference {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:config/circular-inheritance] {detail}")]
    CircularInheritance {
        detail: String,
        context: Option<ErrorContext>,
    },

    #[error("[ail:skill/unknown] {detail}")]
    SkillUnknown {
        detail: String,
        context: Option<ErrorContext>,
    },
}

impl AilError {
    // ── Stable identifier ─────────────────────────────────────────────────────

    /// Return the stable `error_type` string for this error variant.
    ///
    /// This string is serialised into NDJSON output and consumed by downstream
    /// tooling. It must not change across releases.
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::ConfigInvalidYaml { .. } => error_types::CONFIG_INVALID_YAML,
            Self::ConfigFileNotFound { .. } => error_types::CONFIG_FILE_NOT_FOUND,
            Self::ConfigValidationFailed { .. } => error_types::CONFIG_VALIDATION_FAILED,
            Self::TemplateUnresolved { .. } => error_types::TEMPLATE_UNRESOLVED,
            Self::RunnerInvocationFailed { .. } => error_types::RUNNER_INVOCATION_FAILED,
            Self::RunnerCancelled { .. } => error_types::RUNNER_CANCELLED,
            Self::RunnerNotFound { .. } => error_types::RUNNER_NOT_FOUND,
            Self::PipelineAborted { .. } => error_types::PIPELINE_ABORTED,
            Self::StorageQueryFailed { .. } => error_types::STORAGE_QUERY_FAILED,
            Self::RunNotFound { .. } => error_types::RUN_NOT_FOUND,
            Self::StorageDeleteFailed { .. } => error_types::STORAGE_DELETE_FAILED,
            Self::PluginManifestInvalid { .. } => error_types::PLUGIN_MANIFEST_INVALID,
            Self::PluginSpawnFailed { .. } => error_types::PLUGIN_SPAWN_FAILED,
            Self::PluginProtocolError { .. } => error_types::PLUGIN_PROTOCOL_ERROR,
            Self::PluginTimeout { .. } => error_types::PLUGIN_TIMEOUT,
            Self::ConditionInvalid { .. } => error_types::CONDITION_INVALID,
            Self::PipelineCircularReference { .. } => error_types::PIPELINE_CIRCULAR_REFERENCE,
            Self::CircularInheritance { .. } => error_types::CIRCULAR_INHERITANCE,
            Self::SkillUnknown { .. } => error_types::SKILL_UNKNOWN,
        }
    }

    // ── Detail accessors ──────────────────────────────────────────────────────

    /// Borrow the human-readable detail message.
    pub fn detail(&self) -> &str {
        match self {
            Self::ConfigInvalidYaml { detail, .. }
            | Self::ConfigFileNotFound { detail, .. }
            | Self::ConfigValidationFailed { detail, .. }
            | Self::TemplateUnresolved { detail, .. }
            | Self::RunnerInvocationFailed { detail, .. }
            | Self::RunnerCancelled { detail, .. }
            | Self::RunnerNotFound { detail, .. }
            | Self::PipelineAborted { detail, .. }
            | Self::StorageQueryFailed { detail, .. }
            | Self::RunNotFound { detail, .. }
            | Self::StorageDeleteFailed { detail, .. }
            | Self::PluginManifestInvalid { detail, .. }
            | Self::PluginSpawnFailed { detail, .. }
            | Self::PluginProtocolError { detail, .. }
            | Self::PluginTimeout { detail, .. }
            | Self::ConditionInvalid { detail, .. }
            | Self::PipelineCircularReference { detail, .. }
            | Self::CircularInheritance { detail, .. }
            | Self::SkillUnknown { detail, .. } => detail,
        }
    }

    /// Consume this error and return the detail string by value.
    pub fn into_detail(self) -> String {
        match self {
            Self::ConfigInvalidYaml { detail, .. }
            | Self::ConfigFileNotFound { detail, .. }
            | Self::ConfigValidationFailed { detail, .. }
            | Self::TemplateUnresolved { detail, .. }
            | Self::RunnerInvocationFailed { detail, .. }
            | Self::RunnerCancelled { detail, .. }
            | Self::RunnerNotFound { detail, .. }
            | Self::PipelineAborted { detail, .. }
            | Self::StorageQueryFailed { detail, .. }
            | Self::RunNotFound { detail, .. }
            | Self::StorageDeleteFailed { detail, .. }
            | Self::PluginManifestInvalid { detail, .. }
            | Self::PluginSpawnFailed { detail, .. }
            | Self::PluginProtocolError { detail, .. }
            | Self::PluginTimeout { detail, .. }
            | Self::ConditionInvalid { detail, .. }
            | Self::PipelineCircularReference { detail, .. }
            | Self::CircularInheritance { detail, .. }
            | Self::SkillUnknown { detail, .. } => detail,
        }
    }

    // ── Context accessor ──────────────────────────────────────────────────────

    /// Borrow the optional step-scoped context attached to this error.
    pub fn context(&self) -> Option<&ErrorContext> {
        match self {
            Self::ConfigInvalidYaml { context, .. }
            | Self::ConfigFileNotFound { context, .. }
            | Self::ConfigValidationFailed { context, .. }
            | Self::TemplateUnresolved { context, .. }
            | Self::RunnerInvocationFailed { context, .. }
            | Self::RunnerCancelled { context, .. }
            | Self::RunnerNotFound { context, .. }
            | Self::PipelineAborted { context, .. }
            | Self::StorageQueryFailed { context, .. }
            | Self::RunNotFound { context, .. }
            | Self::StorageDeleteFailed { context, .. }
            | Self::PluginManifestInvalid { context, .. }
            | Self::PluginSpawnFailed { context, .. }
            | Self::PluginProtocolError { context, .. }
            | Self::PluginTimeout { context, .. }
            | Self::ConditionInvalid { context, .. }
            | Self::PipelineCircularReference { context, .. }
            | Self::CircularInheritance { context, .. }
            | Self::SkillUnknown { context, .. } => context.as_ref(),
        }
    }

    // ── Context attachment ────────────────────────────────────────────────────

    /// Attach step-scoped context to this error, returning `self`.
    pub fn with_step_context(self, run_id: &str, step_id: &str) -> Self {
        let ctx = Some(ErrorContext::for_step(run_id, step_id));
        match self {
            Self::ConfigInvalidYaml { detail, .. } => Self::ConfigInvalidYaml {
                detail,
                context: ctx,
            },
            Self::ConfigFileNotFound { detail, .. } => Self::ConfigFileNotFound {
                detail,
                context: ctx,
            },
            Self::ConfigValidationFailed { detail, .. } => Self::ConfigValidationFailed {
                detail,
                context: ctx,
            },
            Self::TemplateUnresolved { detail, .. } => Self::TemplateUnresolved {
                detail,
                context: ctx,
            },
            Self::RunnerInvocationFailed { detail, .. } => Self::RunnerInvocationFailed {
                detail,
                context: ctx,
            },
            Self::RunnerCancelled { detail, .. } => Self::RunnerCancelled {
                detail,
                context: ctx,
            },
            Self::RunnerNotFound { detail, .. } => Self::RunnerNotFound {
                detail,
                context: ctx,
            },
            Self::PipelineAborted { detail, .. } => Self::PipelineAborted {
                detail,
                context: ctx,
            },
            Self::StorageQueryFailed { detail, .. } => Self::StorageQueryFailed {
                detail,
                context: ctx,
            },
            Self::RunNotFound { detail, .. } => Self::RunNotFound {
                detail,
                context: ctx,
            },
            Self::StorageDeleteFailed { detail, .. } => Self::StorageDeleteFailed {
                detail,
                context: ctx,
            },
            Self::PluginManifestInvalid { detail, .. } => Self::PluginManifestInvalid {
                detail,
                context: ctx,
            },
            Self::PluginSpawnFailed { detail, .. } => Self::PluginSpawnFailed {
                detail,
                context: ctx,
            },
            Self::PluginProtocolError { detail, .. } => Self::PluginProtocolError {
                detail,
                context: ctx,
            },
            Self::PluginTimeout { detail, .. } => Self::PluginTimeout {
                detail,
                context: ctx,
            },
            Self::ConditionInvalid { detail, .. } => Self::ConditionInvalid {
                detail,
                context: ctx,
            },
            Self::PipelineCircularReference { detail, .. } => Self::PipelineCircularReference {
                detail,
                context: ctx,
            },
            Self::CircularInheritance { detail, .. } => Self::CircularInheritance {
                detail,
                context: ctx,
            },
            Self::SkillUnknown { detail, .. } => Self::SkillUnknown {
                detail,
                context: ctx,
            },
        }
    }

    // ── Recovery hint ─────────────────────────────────────────────────────────

    /// Return a [`RecoveryStrategy`] describing how the caller should react.
    ///
    /// Callers are not required to act on this hint — it is advisory. The executor
    /// does not currently act on recovery strategies; they are intended for future
    /// use in retry loops and step-skip logic.
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        match self {
            Self::RunnerInvocationFailed { .. } => RecoveryStrategy::Retry { max_attempts: 2 },
            Self::PluginSpawnFailed { .. } => RecoveryStrategy::Retry { max_attempts: 2 },
            Self::PluginTimeout { .. } => RecoveryStrategy::Retry { max_attempts: 2 },
            _ => RecoveryStrategy::Abort,
        }
    }

    // ── Constructor helpers ───────────────────────────────────────────────────

    pub fn config_not_found(detail: impl Into<String>) -> Self {
        Self::ConfigFileNotFound {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn config_invalid_yaml(detail: impl Into<String>) -> Self {
        Self::ConfigInvalidYaml {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn config_validation(detail: impl Into<String>) -> Self {
        Self::ConfigValidationFailed {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn template_unresolved(detail: impl Into<String>) -> Self {
        Self::TemplateUnresolved {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn runner_failed(detail: impl Into<String>) -> Self {
        Self::RunnerInvocationFailed {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn runner_cancelled(detail: impl Into<String>) -> Self {
        Self::RunnerCancelled {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn runner_not_found(detail: impl Into<String>) -> Self {
        Self::RunnerNotFound {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn pipeline_aborted(detail: impl Into<String>) -> Self {
        Self::PipelineAborted {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn storage_query_failed(detail: impl Into<String>) -> Self {
        Self::StorageQueryFailed {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn run_not_found(detail: impl Into<String>) -> Self {
        Self::RunNotFound {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn storage_delete_failed(detail: impl Into<String>) -> Self {
        Self::StorageDeleteFailed {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn plugin_manifest_invalid(detail: impl Into<String>) -> Self {
        Self::PluginManifestInvalid {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn plugin_spawn_failed(detail: impl Into<String>) -> Self {
        Self::PluginSpawnFailed {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn plugin_protocol_error(detail: impl Into<String>) -> Self {
        Self::PluginProtocolError {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn plugin_timeout(detail: impl Into<String>) -> Self {
        Self::PluginTimeout {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn condition_invalid(detail: impl Into<String>) -> Self {
        Self::ConditionInvalid {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn pipeline_circular_reference(detail: impl Into<String>) -> Self {
        Self::PipelineCircularReference {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn circular_inheritance(detail: impl Into<String>) -> Self {
        Self::CircularInheritance {
            detail: detail.into(),
            context: None,
        }
    }

    pub fn skill_unknown(detail: impl Into<String>) -> Self {
        Self::SkillUnknown {
            detail: detail.into(),
            context: None,
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
    pub const STORAGE_QUERY_FAILED: &str = "ail:storage/query-failed";
    pub const RUN_NOT_FOUND: &str = "ail:storage/run-not-found";
    pub const STORAGE_DELETE_FAILED: &str = "ail:storage/delete-failed";
    pub const PLUGIN_MANIFEST_INVALID: &str = "ail:plugin/manifest-invalid";
    pub const PLUGIN_SPAWN_FAILED: &str = "ail:plugin/spawn-failed";
    pub const PLUGIN_PROTOCOL_ERROR: &str = "ail:plugin/protocol-error";
    pub const PLUGIN_TIMEOUT: &str = "ail:plugin/timeout";
    pub const CONDITION_INVALID: &str = "ail:condition/invalid";
    pub const PIPELINE_CIRCULAR_REFERENCE: &str = "ail:pipeline/circular-reference";
    pub const CIRCULAR_INHERITANCE: &str = "ail:config/circular-inheritance";
    pub const SKILL_UNKNOWN: &str = "ail:skill/unknown";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_err() -> AilError {
        AilError::ConfigInvalidYaml {
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
        let err = AilError::ConfigValidationFailed {
            detail: "test".to_string(),
            context: None,
        };
        let err = err.with_step_context("run-1", "step-1");
        let ctx = err.context().unwrap();
        assert_eq!(ctx.pipeline_run_id, Some("run-1".to_string()));
        assert_eq!(ctx.step_id, Some("step-1".to_string()));
        assert!(ctx.source.is_none());
    }

    #[test]
    fn none_context_does_not_affect_display() {
        let err_with = AilError::ConfigInvalidYaml {
            detail: "some detail".to_string(),
            context: Some(ErrorContext {
                pipeline_run_id: Some("run-1".to_string()),
                step_id: None,
                source: None,
            }),
        };
        let err_without = AilError::ConfigInvalidYaml {
            detail: "some detail".to_string(),
            context: None,
        };
        assert_eq!(err_with.to_string(), err_without.to_string());
    }

    #[test]
    fn constructors_set_correct_error_type() {
        assert_eq!(
            AilError::config_not_found("d").error_type(),
            error_types::CONFIG_FILE_NOT_FOUND
        );
        assert_eq!(
            AilError::config_invalid_yaml("d").error_type(),
            error_types::CONFIG_INVALID_YAML
        );
        assert_eq!(
            AilError::config_validation("d").error_type(),
            error_types::CONFIG_VALIDATION_FAILED
        );
        assert_eq!(
            AilError::template_unresolved("d").error_type(),
            error_types::TEMPLATE_UNRESOLVED
        );
        assert_eq!(
            AilError::runner_failed("d").error_type(),
            error_types::RUNNER_INVOCATION_FAILED
        );
        assert_eq!(
            AilError::runner_not_found("d").error_type(),
            error_types::RUNNER_NOT_FOUND
        );
        assert_eq!(
            AilError::pipeline_aborted("d").error_type(),
            error_types::PIPELINE_ABORTED
        );
        assert_eq!(
            AilError::runner_cancelled("d").error_type(),
            error_types::RUNNER_CANCELLED
        );
    }

    #[test]
    fn recovery_strategy_runner_failed_is_retry() {
        let err = AilError::runner_failed("d");
        assert_eq!(
            err.recovery_strategy(),
            RecoveryStrategy::Retry { max_attempts: 2 }
        );
    }

    #[test]
    fn recovery_strategy_config_errors_abort() {
        assert_eq!(
            AilError::config_validation("d").recovery_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            AilError::config_not_found("d").recovery_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            AilError::config_invalid_yaml("d").recovery_strategy(),
            RecoveryStrategy::Abort
        );
    }

    #[test]
    fn detail_accessor_returns_correct_string() {
        let err = AilError::TemplateUnresolved {
            detail: "missing var".to_string(),
            context: None,
        };
        assert_eq!(err.detail(), "missing var");
    }

    #[test]
    fn into_detail_consumes_and_returns_string() {
        let err = AilError::PipelineAborted {
            detail: "aborted reason".to_string(),
            context: None,
        };
        assert_eq!(err.into_detail(), "aborted reason");
    }
}
