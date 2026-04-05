/// SPEC §9 — Sub-pipeline execution and §11 template variables in pipeline: paths.
use ail_core::config::domain::{
    Pipeline, ResultAction, ResultBranch, ResultMatcher, Step, StepBody, StepId,
};
use ail_core::error::error_types;
use ail_core::executor::{execute, execute_with_control, ExecutionControl};
use ail_core::runner::stub::StubRunner;
use ail_core::session::Session;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn make_session(steps: Vec<Step>) -> Session {
    let pipeline = Pipeline {
        steps,
        source: None,
        defaults: Default::default(),
    };
    Session::new(pipeline, "invocation prompt".to_string())
}

fn prompt_step(id: &str, text: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
    }
}

fn sub_pipeline_step(id: &str, path: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::SubPipeline(path.to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
    }
}

// ── §9.1 Basic sub-pipeline execution ──────────────────────────────────────

#[test]
fn sub_pipeline_step_loads_and_executes_child_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    let mut session = make_session(vec![sub_pipeline_step(
        "call_child",
        child_path.to_str().unwrap(),
    )]);

    let runner = StubRunner::new("child response");
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    // The sub-pipeline produces one TurnEntry for the calling step
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "call_child");
    assert!(
        entries[0].response.is_some(),
        "Expected response from sub-pipeline"
    );

    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn sub_pipeline_response_is_accessible_as_template_variable() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    let mut session = make_session(vec![
        sub_pipeline_step("delegate", child_path.to_str().unwrap()),
        prompt_step("followup", "Result was: {{ step.delegate.response }}"),
    ]);

    let runner = StubRunner::new("stub");
    execute(&mut session, &runner).unwrap();

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    // The followup prompt should have the delegate response interpolated
    assert!(
        entries[1].prompt.starts_with("Result was:"),
        "Expected prompt with interpolated response, got: {}",
        entries[1].prompt
    );

    std::env::set_current_dir(orig).unwrap();
}

// ── §11 Template variables in pipeline: paths ──────────────────────────────

#[test]
fn pipeline_path_with_template_variable_resolves_at_runtime() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    // Selector step outputs the child pipeline path; delegate step uses a template var
    let selector = prompt_step("selector", "select a pipeline");
    let delegate = sub_pipeline_step("delegate", "{{ step.selector.response }}");
    let mut session = make_session(vec![selector, delegate]);

    // StubRunner returns the child path as the response to the selector step
    let runner = StubRunner::new(child_path.to_str().unwrap());
    let result = execute(&mut session, &runner);
    assert!(
        result.is_ok(),
        "Expected sub-pipeline via template var to succeed: {result:?}"
    );

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2, "Expected selector + delegate entries");
    assert_eq!(entries[0].step_id, "selector");
    assert_eq!(entries[1].step_id, "delegate");

    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn unresolvable_pipeline_path_aborts_with_template_unresolved_error() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let delegate = sub_pipeline_step("delegate", "{{ step.nonexistent.response }}");
    let mut session = make_session(vec![delegate]);

    let runner = StubRunner::new("stub");
    let result = execute(&mut session, &runner);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type,
        ail_core::error::error_types::TEMPLATE_UNRESOLVED
    );

    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn missing_sub_pipeline_file_aborts_with_file_not_found_error() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let delegate = sub_pipeline_step("delegate", "./does_not_exist.ail.yaml");
    let mut session = make_session(vec![delegate]);

    let runner = StubRunner::new("stub");
    let result = execute(&mut session, &runner);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type,
        ail_core::error::error_types::CONFIG_FILE_NOT_FOUND
    );

    std::env::set_current_dir(orig).unwrap();
}

// ── on_result: pipeline: action ────────────────────────────────────────────

#[test]
fn on_result_pipeline_action_executes_sub_pipeline_on_match() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    let trigger = Step {
        id: StepId("trigger".to_string()),
        body: StepBody::Prompt("trigger prompt".to_string()),
        message: None,
        tools: None,
        on_result: Some(vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: ResultAction::Pipeline(child_path.to_str().unwrap().to_string()),
        }]),
        model: None,
        runner: None,
    };
    let mut session = make_session(vec![trigger]);

    let runner = StubRunner::new("stub response");
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    // Expect entries: trigger step + sub-pipeline result appended to turn log
    let entries = session.turn_log.entries();
    assert!(
        entries.len() >= 2,
        "Expected trigger + sub-pipeline entries, got {}",
        entries.len()
    );
    assert_eq!(entries[0].step_id, "trigger");

    std::env::set_current_dir(orig).unwrap();
}

// ── Validation: pipeline: action parses correctly ──────────────────────────

#[test]
fn pipeline_action_in_on_result_parses_from_yaml() {
    use ail_core::config::load;

    // Write a temporary pipeline YAML with on_result: pipeline: action
    let tmp = tempfile::tempdir().unwrap();
    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    let yaml = format!(
        r#"
version: "0.0.1"
pipeline:
  - id: router
    prompt: "classify"
    on_result:
      - always: true
        action: "pipeline: {}"
"#,
        child_path.display()
    );
    let yaml_path = tmp.path().join("test.ail.yaml");
    std::fs::write(&yaml_path, yaml).unwrap();

    let pipeline = load(&yaml_path).unwrap();
    let branches = pipeline.steps[0].on_result.as_ref().unwrap();
    assert!(matches!(branches[0].action, ResultAction::Pipeline(_)));
}

#[test]
fn pipeline_action_missing_path_is_validation_error() {
    let tmp = tempfile::tempdir().unwrap();
    let yaml = r#"
version: "0.0.1"
pipeline:
  - id: step
    prompt: "test"
    on_result:
      - always: true
        action: "pipeline:"
"#;
    let yaml_path = tmp.path().join("bad.ail.yaml");
    std::fs::write(&yaml_path, yaml).unwrap();

    let result = ail_core::config::load(&yaml_path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type,
        ail_core::error::error_types::CONFIG_VALIDATION_FAILED
    );
}

// ── execute_with_control: sub-pipeline depth tracking ──────────────────────

/// Regression test: execute_with_control must pass depth=1 (not 0) to
/// execute_sub_pipeline so that the MAX_SUB_PIPELINE_DEPTH guard fires
/// correctly on the controlled (TUI/json) code path.
#[test]
fn execute_with_control_sub_pipeline_runs_and_records_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    let mut session = make_session(vec![sub_pipeline_step(
        "call_child",
        child_path.to_str().unwrap(),
    )]);

    let runner = StubRunner::new("child response");
    let control = ExecutionControl::new();
    let disabled = HashSet::new();
    let (event_tx, _event_rx) = mpsc::channel();
    let (_hitl_tx, hitl_rx) = mpsc::channel();

    let result = execute_with_control(
        &mut session,
        &runner,
        &control,
        &disabled,
        event_tx,
        hitl_rx,
    );
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "call_child");
    assert!(
        entries[0].response.is_some(),
        "Expected response from sub-pipeline"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// Verify that the MAX_SUB_PIPELINE_DEPTH guard is enforced on the
/// execute_with_control code path. Two temporary pipeline files are written
/// that call each other, creating an infinite cycle. The depth guard must
/// abort with PIPELINE_ABORTED before the stack overflows.
#[test]
fn execute_with_control_depth_limit_prevents_infinite_recursion() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Write two mutually-recursive pipelines: a.ail.yaml calls b.ail.yaml
    // and b.ail.yaml calls a.ail.yaml.
    let path_a = tmp.path().join("a.ail.yaml");
    let path_b = tmp.path().join("b.ail.yaml");

    let yaml_a = format!(
        "version: \"0.0.1\"\npipeline:\n  - id: recurse_a\n    pipeline: {}\n",
        path_b.display()
    );
    let yaml_b = format!(
        "version: \"0.0.1\"\npipeline:\n  - id: recurse_b\n    pipeline: {}\n",
        path_a.display()
    );
    std::fs::write(&path_a, yaml_a).unwrap();
    std::fs::write(&path_b, yaml_b).unwrap();

    let mut session = make_session(vec![sub_pipeline_step("root", path_a.to_str().unwrap())]);

    let runner = StubRunner::new("stub");
    let control = ExecutionControl::new();
    let disabled = HashSet::new();
    let (event_tx, _event_rx) = mpsc::channel();
    let (_hitl_tx, hitl_rx) = mpsc::channel();

    let result = execute_with_control(
        &mut session,
        &runner,
        &control,
        &disabled,
        event_tx,
        hitl_rx,
    );
    assert!(
        result.is_err(),
        "Expected depth limit error, got: {result:?}"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type,
        error_types::PIPELINE_ABORTED,
        "Expected PIPELINE_ABORTED for depth limit, got: {}",
        err.error_type
    );
    assert!(
        err.detail.contains("depth"),
        "Error detail should mention depth: {}",
        err.detail
    );

    std::env::set_current_dir(orig).unwrap();
}

/// Same depth-limit test on the execute() (simple) code path, for parity.
#[test]
fn execute_depth_limit_prevents_infinite_recursion() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let path_a = tmp.path().join("da.ail.yaml");
    let path_b = tmp.path().join("db.ail.yaml");

    let yaml_a = format!(
        "version: \"0.0.1\"\npipeline:\n  - id: recurse_a\n    pipeline: {}\n",
        path_b.display()
    );
    let yaml_b = format!(
        "version: \"0.0.1\"\npipeline:\n  - id: recurse_b\n    pipeline: {}\n",
        path_a.display()
    );
    std::fs::write(&path_a, yaml_a).unwrap();
    std::fs::write(&path_b, yaml_b).unwrap();

    let mut session = make_session(vec![sub_pipeline_step("root", path_a.to_str().unwrap())]);

    let runner = StubRunner::new("stub");
    let result = execute(&mut session, &runner);
    assert!(
        result.is_err(),
        "Expected depth limit error, got: {result:?}"
    );
    let err = result.unwrap_err();
    assert_eq!(err.error_type, error_types::PIPELINE_ABORTED);

    std::env::set_current_dir(orig).unwrap();
}
