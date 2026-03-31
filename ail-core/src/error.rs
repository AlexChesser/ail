use std::fmt;

pub struct AilError {
    pub error_type: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub context: Option<ErrorContext>,
}

pub struct ErrorContext {
    pub pipeline_run_id: Option<String>,
    pub step_id: Option<String>,
    pub source: Option<String>,
}

impl fmt::Display for AilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.error_type, self.title, self.detail)
    }
}

impl fmt::Debug for AilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::error::Error for AilError {}

pub mod error_types {
    pub const CONFIG_INVALID_YAML: &str = "ail:config/invalid-yaml";
    pub const CONFIG_FILE_NOT_FOUND: &str = "ail:config/file-not-found";
    pub const CONFIG_VALIDATION_FAILED: &str = "ail:config/validation-failed";
    pub const TEMPLATE_UNRESOLVED: &str = "ail:template/unresolved-variable";
    pub const RUNNER_INVOCATION_FAILED: &str = "ail:runner/invocation-failed";
    pub const RUNNER_CANCELLED: &str = "ail:runner/cancelled";
    pub const PIPELINE_ABORTED: &str = "ail:pipeline/aborted";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_contains_error_type() {
        let err = AilError {
            error_type: error_types::CONFIG_INVALID_YAML,
            title: "Invalid YAML",
            detail: "unexpected token at line 3".to_string(),
            context: None,
        };
        assert!(err.to_string().contains(error_types::CONFIG_INVALID_YAML));
    }

    #[test]
    fn display_contains_detail() {
        let err = AilError {
            error_type: error_types::CONFIG_INVALID_YAML,
            title: "Invalid YAML",
            detail: "unexpected token at line 3".to_string(),
            context: None,
        };
        assert!(err.to_string().contains("unexpected token at line 3"));
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
}
