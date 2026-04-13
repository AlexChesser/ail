/// SPEC §19 — RunnerFactory: build by name, AIL_DEFAULT_RUNNER env var, RUNNER_NOT_FOUND error.
use ail_core::config::domain::ProviderConfig;
use ail_core::error::error_types;
use ail_core::runner::factory::RunnerFactory;
use ail_core::runner::http::HttpSessionStore;
use ail_core::runner::{InvokeOptions, Runner};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn test_store() -> HttpSessionStore {
    Arc::new(Mutex::new(HashMap::new()))
}

fn test_provider() -> ProviderConfig {
    ProviderConfig::default()
}

fn build(name: &str) -> Result<Box<dyn Runner + Send>, ail_core::error::AilError> {
    RunnerFactory::build(name, false, &test_store(), &test_provider())
}

/// Factory builds a stub runner by name.
#[test]
fn factory_builds_stub_runner_by_name() {
    let runner = build("stub").expect("stub runner should build");
    let result = runner
        .invoke("hello", InvokeOptions::default())
        .expect("stub runner should succeed");
    assert_eq!(result.response, "stub response");
}

/// Factory builds a stub runner with trailing whitespace and mixed case.
#[test]
fn factory_builds_stub_runner_case_insensitive_and_trimmed() {
    let runner = build("  Stub  ").expect("should build with whitespace/case");
    let result = runner
        .invoke("hello", InvokeOptions::default())
        .expect("stub runner should succeed");
    assert_eq!(result.response, "stub response");
}

/// Factory returns RUNNER_NOT_FOUND for an unknown runner name.
#[test]
fn factory_returns_runner_not_found_for_unknown_name() {
    let result = build("nonexistent");
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(
        err.error_type(),
        error_types::RUNNER_NOT_FOUND,
        "expected RUNNER_NOT_FOUND, got: {}",
        err.error_type()
    );
    assert!(
        err.detail().contains("nonexistent"),
        "detail should contain the unknown name: {}",
        err.detail()
    );
}

/// Factory builds a stub runner object that satisfies the Runner trait.
#[test]
fn factory_stub_runner_satisfies_runner_trait() {
    let runner: Box<dyn Runner> = build("stub").unwrap();
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
    let result = build("claude");
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
    let result = build("claude");
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

/// Factory builds an HTTP runner by name (object construction only — no live server needed).
#[test]
fn factory_builds_http_runner_by_name() {
    let result = build("http");
    assert!(
        result.is_ok(),
        "http runner construction should succeed: {:?}",
        result.err()
    );
}

/// "ollama" is an alias for "http" — both resolve to the same HttpRunner.
#[test]
fn factory_builds_ollama_runner_as_http_alias() {
    let result = build("ollama");
    assert!(
        result.is_ok(),
        "ollama alias should construct successfully: {:?}",
        result.err()
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

/// Executor dispatches per-step runner when `runner:` field is set on a step.
///
/// Step A uses the injected default runner (returns "default response").
/// Step B overrides with `runner: "stub"` — RunnerFactory builds a StubRunner
/// that returns "stub response". Verifies each step records the correct response.
#[test]
fn executor_dispatches_per_step_runner() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    use ail_core::config::domain::{Pipeline, Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::session::Session;

    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Step A: no runner field — uses injected default.
    let step_a = Step {
        id: StepId("step_a".to_string()),
        body: StepBody::Prompt("prompt A".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
    };
    // Step B: runner: "stub" — executor builds StubRunner via RunnerFactory.
    let step_b = Step {
        id: StepId("step_b".to_string()),
        body: StepBody::Prompt("prompt B".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: Some("stub".to_string()),
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
    };

    let pipeline = Pipeline {
        steps: vec![step_a, step_b],
        source: None,
        defaults: Default::default(),
        timeout_seconds: None,
        default_tools: None,
        named_pipelines: Default::default(),
    };

    // The default runner returns a distinct response so we can tell it apart from the stub.
    let default_runner = StubRunner::new("default response");
    let mut session = Session::new(pipeline, "invocation prompt".to_string());

    execute(&mut session, &default_runner).unwrap();

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);

    // Step A used the default runner.
    assert_eq!(entries[0].step_id, "step_a");
    assert_eq!(
        entries[0].response.as_deref(),
        Some("default response"),
        "step_a should use the injected default runner"
    );

    // Step B used the per-step stub runner (RunnerFactory::build("stub", true) returns "stub response").
    assert_eq!(entries[1].step_id, "step_b");
    assert_eq!(
        entries[1].response.as_deref(),
        Some("stub response"),
        "step_b should use the per-step stub runner"
    );

    std::env::set_current_dir(orig).unwrap();
}
