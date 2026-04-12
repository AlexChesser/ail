//! End-to-end tests for `ail validate`.

mod common;

use predicates::prelude::*;

#[test]
fn validate_valid_pipeline_text_success() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["validate", "--pipeline"])
        .arg(common::fixture_path("minimal.ail.yaml"));

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Pipeline valid:"));
}

#[test]
fn validate_valid_pipeline_json_success() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["validate", "--pipeline"])
        .arg(common::fixture_path("minimal.ail.yaml"))
        .args(["--output-format", "json"]);

    let output = cmd.output().expect("failed to run ail validate");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["valid"], true);
    assert!(
        json["step_count"].as_u64().is_some(),
        "Expected step_count as number, got: {json}"
    );
}

#[test]
fn validate_invalid_pipeline_text_stderr_exit_1() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["validate", "--pipeline"])
        .arg(common::fixture_path("invalid_duplicate_ids.ail.yaml"));

    cmd.assert()
        .failure()
        .stderr(predicates::str::is_empty().not());
}

#[test]
fn validate_invalid_pipeline_json_error_type() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["validate", "--pipeline"])
        .arg(common::fixture_path("invalid_duplicate_ids.ail.yaml"))
        .args(["--output-format", "json"]);

    let output = cmd.output().expect("failed to run ail validate");
    assert!(!output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["valid"], false);
    let errors = json["errors"].as_array().expect("errors should be array");
    assert!(!errors.is_empty());
    let error_type = errors[0]["error_type"]
        .as_str()
        .expect("error_type should be string");
    assert!(
        error_type.starts_with("ail:config/"),
        "Expected error_type starting with 'ail:config/', got: {error_type}"
    );
}

#[test]
fn validate_missing_pipeline_exits_1() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["validate", "--pipeline", "/nonexistent/path.ail.yaml"]);

    cmd.assert().failure();
}
