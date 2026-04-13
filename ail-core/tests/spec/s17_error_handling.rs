use ail_core::error::{error_types, AilError, ErrorContext};

/// SPEC §17 — errors carry a stable type string and instance detail
#[test]
fn ail_error_display_contains_type_and_detail() {
    let err = AilError::RunnerInvocationFailed {
        detail: "process exited with code 1".to_string(),
        context: Some(ErrorContext {
            pipeline_run_id: Some("run-abc".to_string()),
            step_id: Some("dont_be_stupid".to_string()),
            source: Some("exit status: 1".to_string()),
        }),
    };
    let display = err.to_string();
    assert!(display.contains(error_types::RUNNER_INVOCATION_FAILED));
    assert!(display.contains("process exited with code 1"));
}

/// SPEC §17, §6 — skill: steps with a valid name parse successfully.
/// Unknown skill names are detected at execution time, not parse time.
#[test]
fn skill_step_parses_successfully() {
    use ail_core::config;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"0.0.1\"\npipeline:\n  - id: my_skill\n    skill: ail/code_review\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let result = config::load(&path);
    assert!(result.is_ok(), "skill: step should parse successfully");
}

/// SPEC §17 — skill: step with empty name is rejected at validation time.
#[test]
fn skill_step_empty_name_rejected_at_validation_time() {
    use ail_core::config;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"0.0.1\"\npipeline:\n  - id: my_skill\n    skill: \"  \"\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let result = config::load(&path);
    assert!(
        result.is_err(),
        "skill: step with empty name must be rejected at load time"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type(),
        error_types::CONFIG_VALIDATION_FAILED,
        "empty skill name must produce CONFIG_VALIDATION_FAILED, got: {}",
        err.error_type()
    );
}

/// SPEC §17 — unknown skill name produces SKILL_UNKNOWN at execution time.
#[test]
fn unknown_skill_produces_skill_unknown_error() {
    let err = AilError::skill_unknown("Unknown skill 'ail/nonexistent'");
    assert_eq!(err.error_type(), error_types::SKILL_UNKNOWN);
    assert!(err.detail().contains("nonexistent"));
}

/// SPEC §17, #76 — storage query failures produce STORAGE_QUERY_FAILED, not PIPELINE_ABORTED
#[test]
fn storage_error_types_are_distinct_from_pipeline_aborted() {
    let err = AilError::storage_query_failed("connection refused");
    assert_eq!(err.error_type(), error_types::STORAGE_QUERY_FAILED);
    assert_ne!(
        err.error_type(),
        error_types::PIPELINE_ABORTED,
        "storage query failures must not reuse PIPELINE_ABORTED"
    );

    let err = AilError::run_not_found("run abc not found");
    assert_eq!(err.error_type(), error_types::RUN_NOT_FOUND);

    let err = AilError::storage_delete_failed("disk full");
    assert_eq!(err.error_type(), error_types::STORAGE_DELETE_FAILED);
}
