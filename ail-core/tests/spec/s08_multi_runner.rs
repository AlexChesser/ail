/// SPEC §19 — RunnerFactory: build by name, AIL_DEFAULT_RUNNER env var, RUNNER_NOT_FOUND error.
use ail_core::error::error_types;
use ail_core::runner::factory::RunnerFactory;
use ail_core::runner::{InvokeOptions, Runner};

/// Factory builds a stub runner by name.
#[test]
fn factory_builds_stub_runner_by_name() {
    let runner = RunnerFactory::build("stub", false).expect("stub runner should build");
    let result = runner
        .invoke("hello", InvokeOptions::default())
        .expect("stub runner should succeed");
    assert_eq!(result.response, "stub response");
}

/// Factory builds a stub runner with trailing whitespace and mixed case.
#[test]
fn factory_builds_stub_runner_case_insensitive_and_trimmed() {
    let runner =
        RunnerFactory::build("  Stub  ", false).expect("should build with whitespace/case");
    let result = runner
        .invoke("hello", InvokeOptions::default())
        .expect("stub runner should succeed");
    assert_eq!(result.response, "stub response");
}

/// Factory returns RUNNER_NOT_FOUND for an unknown runner name.
#[test]
fn factory_returns_runner_not_found_for_unknown_name() {
    let result = RunnerFactory::build("nonexistent", false);
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(
        err.error_type,
        error_types::RUNNER_NOT_FOUND,
        "expected RUNNER_NOT_FOUND, got: {}",
        err.error_type
    );
    assert!(
        err.detail.contains("nonexistent"),
        "detail should contain the unknown name: {}",
        err.detail
    );
}

/// Factory builds a stub runner object that satisfies the Runner trait.
#[test]
fn factory_stub_runner_satisfies_runner_trait() {
    let runner: Box<dyn Runner> = RunnerFactory::build("stub", false).unwrap();
    let result = runner
        .invoke("test prompt", InvokeOptions::default())
        .unwrap();
    assert!(!result.response.is_empty());
    assert!(result.session_id.is_some());
    assert!(result.cost_usd.is_some());
}

/// Factory builds the claude runner by name (object construction only — no subprocess).
#[test]
fn factory_builds_claude_runner_by_name() {
    // Only tests that construction succeeds. Invoking the runner requires the claude binary.
    let result = RunnerFactory::build("claude", false);
    assert!(
        result.is_ok(),
        "claude runner construction should succeed: {:?}",
        result.err()
    );
}

/// build_default() falls back to claude when AIL_DEFAULT_RUNNER is not set.
/// Tests the build("claude") path as a proxy for the env fallback.
#[test]
fn build_default_claude_fallback_constructs_successfully() {
    // We verify that the "claude" fallback path builds without error.
    // Actual env-var reading is tested in factory unit tests.
    let result = RunnerFactory::build("claude", false);
    assert!(result.is_ok());
}

/// Per-step runner field is parsed from YAML and preserved in domain type.
#[test]
fn per_step_runner_field_parsed_from_yaml() {
    use std::io::Write;
    let yaml = r#"
version: "1"
pipeline:
  - id: step_with_runner
    prompt: "hello"
    runner: stub
"#;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.as_file().write_all(yaml.as_bytes()).unwrap();
    let pipeline = ail_core::config::load(tmp.path()).unwrap();
    let step = pipeline.steps.first().unwrap();
    assert_eq!(
        step.runner.as_deref(),
        Some("stub"),
        "runner field should be 'stub', got: {:?}",
        step.runner
    );
}

/// Per-step runner field defaults to None when not specified in YAML.
#[test]
fn per_step_runner_field_defaults_to_none() {
    use std::io::Write;
    let yaml = r#"
version: "1"
pipeline:
  - id: step_without_runner
    prompt: "hello"
"#;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.as_file().write_all(yaml.as_bytes()).unwrap();
    let pipeline = ail_core::config::load(tmp.path()).unwrap();
    let step = pipeline.steps.first().unwrap();
    assert!(
        step.runner.is_none(),
        "runner field should be None when not set, got: {:?}",
        step.runner
    );
}
