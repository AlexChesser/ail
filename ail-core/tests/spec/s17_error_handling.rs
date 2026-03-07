use ail_core::error::{error_types, AilError, ErrorContext};

/// SPEC §17 — errors carry a stable type string and instance detail
#[test]
fn ail_error_display_contains_type_and_detail() {
    let err = AilError {
        error_type: error_types::RUNNER_INVOCATION_FAILED,
        title: "Runner invocation failed",
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
