/// SPEC §9 — Sub-pipeline execution and §11 template variables in pipeline: paths.
use ail_core::config::domain::{ResultAction, ResultBranch, ResultMatcher, Step, StepBody, StepId};
use ail_core::error::error_types;
use ail_core::executor::{execute, execute_with_control, ExecutionControl};
use ail_core::runner::stub::{EchoStubRunner, StubRunner};
use ail_core::session::Session;
use ail_core::test_helpers::{make_session, prompt_step};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn sub_pipeline_step(id: &str, path: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::SubPipeline {
            path: path.to_string(),
            prompt: None,
        },
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
    }
}

// ── §9.1 Basic sub-pipeline execution ──────────────────────────────────────

#[test]
fn sub_pipeline_step_loads_and_executes_child_pipeline() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
        err.error_type(),
        ail_core::error::error_types::TEMPLATE_UNRESOLVED
    );

    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn missing_sub_pipeline_file_aborts_with_file_not_found_error() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
        err.error_type(),
        ail_core::error::error_types::CONFIG_FILE_NOT_FOUND
    );

    std::env::set_current_dir(orig).unwrap();
}

// ── on_result: pipeline: action ────────────────────────────────────────────

#[test]
fn on_result_pipeline_action_executes_sub_pipeline_on_match() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
            action: ResultAction::Pipeline {
                path: child_path.to_str().unwrap().to_string(),
                prompt: None,
            },
        }]),
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
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
    assert!(matches!(branches[0].action, ResultAction::Pipeline { .. }));
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
        err.error_type(),
        ail_core::error::error_types::CONFIG_VALIDATION_FAILED
    );
}

// ── execute_with_control: sub-pipeline depth tracking ──────────────────────

/// Regression test: execute_with_control must pass depth=1 (not 0) to
/// execute_sub_pipeline so that the MAX_SUB_PIPELINE_DEPTH guard fires
/// correctly on the controlled (TUI/json) code path.
#[test]
fn execute_with_control_sub_pipeline_runs_and_records_entry() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
        err.error_type(),
        error_types::PIPELINE_ABORTED,
        "Expected PIPELINE_ABORTED for depth limit, got: {}",
        err.error_type()
    );
    assert!(
        err.detail().contains("depth"),
        "Error detail should mention depth: {}",
        err.detail()
    );

    std::env::set_current_dir(orig).unwrap();
}

/// Same depth-limit test on the execute() (simple) code path, for parity.
#[test]
fn execute_depth_limit_prevents_infinite_recursion() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
    assert_eq!(err.error_type(), error_types::PIPELINE_ABORTED);

    std::env::set_current_dir(orig).unwrap();
}

// ── §9 prompt override: sub-pipeline receives explicit invocation prompt ──────

/// When `prompt:` is set on a `pipeline:` step, the child session's invocation
/// prompt must be the resolved prompt value, not the parent's last response.
#[test]
fn sub_pipeline_step_prompt_override_is_passed_to_child() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = tmp.path().join("child_override.ail.yaml");
    std::fs::write(
        &child_path,
        "version: \"0.0.1\"\npipeline:\n  - id: echo\n    prompt: \"{{ step.invocation.prompt }}\"\n",
    )
    .unwrap();

    let step = Step {
        id: StepId("parent".to_string()),
        body: StepBody::SubPipeline {
            path: child_path.to_str().unwrap().to_string(),
            prompt: Some("explicit override".to_string()),
        },
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
    };
    let mut session = make_session(vec![step]);
    // Add a prior turn entry so we can confirm it is NOT used as the child prompt.
    session.turn_log.append(ail_core::session::TurnEntry {
        step_id: "prior".to_string(),
        prompt: "prior prompt".to_string(),
        response: Some("prior response — should be ignored".to_string()),
        ..Default::default()
    });

    let runner = EchoStubRunner::new();
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    // The sub-pipeline echoes its invocation_prompt; verify "explicit override" reached it.
    let last = session.turn_log.last_response().unwrap_or("");
    assert!(
        last.contains("explicit override"),
        "Expected response to contain 'explicit override', got: {last:?}"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// `prompt:` with template variables is resolved against the parent session.
#[test]
fn sub_pipeline_step_prompt_override_resolves_template_variables() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = tmp.path().join("child_tmpl.ail.yaml");
    std::fs::write(
        &child_path,
        "version: \"0.0.1\"\npipeline:\n  - id: echo\n    prompt: \"{{ step.invocation.prompt }}\"\n",
    )
    .unwrap();

    let step = Step {
        id: StepId("parent".to_string()),
        body: StepBody::SubPipeline {
            path: child_path.to_str().unwrap().to_string(),
            prompt: Some("{{ step.invocation.prompt }}".to_string()),
        },
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
    };
    // make_session sets invocation_prompt to "invocation prompt"
    let mut session = make_session(vec![step]);

    let runner = EchoStubRunner::new();
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let last = session.turn_log.last_response().unwrap_or("");
    assert!(
        last.contains("invocation prompt"),
        "Expected resolved template variable 'invocation prompt', got: {last:?}"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// `on_result: pipeline:` with `prompt:` sends the specified prompt to the child.
#[test]
fn on_result_pipeline_prompt_override_is_passed_to_child() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let child_path = tmp.path().join("child_on_result.ail.yaml");
    std::fs::write(
        &child_path,
        "version: \"0.0.1\"\npipeline:\n  - id: echo\n    prompt: \"{{ step.invocation.prompt }}\"\n",
    )
    .unwrap();

    let trigger = Step {
        id: StepId("trigger".to_string()),
        body: StepBody::Prompt("trigger prompt".to_string()),
        message: None,
        tools: None,
        on_result: Some(vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: ResultAction::Pipeline {
                path: child_path.to_str().unwrap().to_string(),
                prompt: Some("routed prompt".to_string()),
            },
        }]),
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
    };
    let mut session = make_session(vec![trigger]);

    let runner = EchoStubRunner::new();
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let last = session.turn_log.last_response().unwrap_or("");
    assert!(
        last.contains("routed prompt"),
        "Expected last response to contain 'routed prompt', got: {last:?}"
    );

    std::env::set_current_dir(orig).unwrap();
}

// ── §9 relative sub-pipeline path resolution ─────────────────────────────────

/// Sub-pipeline paths starting with ./ or ../ must be resolved relative to the
/// parent pipeline file's directory, not the process CWD (SPEC §9).
#[test]
fn sub_pipeline_relative_path_resolves_relative_to_parent_pipeline_dir() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();

    // Write the child pipeline into a subdirectory of the temp dir.
    let subdir = tmp.path().join("workflows");
    std::fs::create_dir_all(&subdir).unwrap();
    let child_path = subdir.join("child.ail.yaml");
    std::fs::write(
        &child_path,
        "version: \"0.0.1\"\npipeline:\n  - id: inner\n    prompt: \"child ran\"\n",
    )
    .unwrap();

    // Write a parent pipeline file that references the child via ./workflows/child.ail.yaml.
    let parent_yaml = "version: \"0.0.1\"\npipeline:\n  - id: call_child\n    pipeline: ./workflows/child.ail.yaml\n";
    let parent_path = tmp.path().join("parent.ail.yaml");
    std::fs::write(&parent_path, parent_yaml).unwrap();

    // Load the parent pipeline (sets pipeline.source so base_dir is resolved correctly).
    let pipeline = ail_core::config::load(&parent_path).unwrap();

    // Change CWD to a completely different directory so the relative path would fail
    // if resolved against CWD instead of the pipeline file's directory.
    let orig = std::env::current_dir().unwrap();
    let unrelated = tempfile::tempdir().unwrap();
    std::env::set_current_dir(unrelated.path()).unwrap();

    let mut session = Session::new(pipeline, "hello".to_string());
    let runner = StubRunner::new("child response");
    let result = execute(&mut session, &runner);

    std::env::set_current_dir(orig).unwrap();

    assert!(
        result.is_ok(),
        "Expected relative sub-pipeline to resolve from parent dir, got: {result:?}"
    );
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "call_child");
}

/// `on_result: pipeline:` with a ./relative path resolves relative to the parent pipeline file.
#[test]
fn on_result_pipeline_relative_path_resolves_relative_to_parent_pipeline_dir() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();

    let subdir = tmp.path().join("handlers");
    std::fs::create_dir_all(&subdir).unwrap();
    let child_path = subdir.join("handler.ail.yaml");
    std::fs::write(
        &child_path,
        "version: \"0.0.1\"\npipeline:\n  - id: handle\n    prompt: \"handled\"\n",
    )
    .unwrap();

    let parent_yaml = "version: \"0.0.1\"\npipeline:\n  - id: trigger\n    prompt: \"go\"\n    on_result:\n      - always: true\n        action: \"pipeline: ./handlers/handler.ail.yaml\"\n";
    let parent_path = tmp.path().join("parent2.ail.yaml");
    std::fs::write(&parent_path, parent_yaml).unwrap();

    let pipeline = ail_core::config::load(&parent_path).unwrap();

    let orig = std::env::current_dir().unwrap();
    let unrelated = tempfile::tempdir().unwrap();
    std::env::set_current_dir(unrelated.path()).unwrap();

    let mut session = Session::new(pipeline, "hello".to_string());
    let runner = StubRunner::new("stub");
    let result = execute(&mut session, &runner);

    std::env::set_current_dir(orig).unwrap();

    assert!(
        result.is_ok(),
        "Expected on_result relative pipeline path to resolve from parent dir, got: {result:?}"
    );
    let entries = session.turn_log.entries();
    assert!(entries.len() >= 2, "Expected trigger + handler entries");
}

/// When a pipeline is discovered or loaded as a bare filename (no directory component),
/// parent() returns "" — an empty base must not break sub-pipeline path resolution.
/// This is the bare-filename variant of the §9 relative-path tests.
#[test]
fn sub_pipeline_relative_path_resolves_when_pipeline_loaded_as_bare_filename() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();

    // Write child pipeline in a subdirectory.
    let subdir = tmp.path().join("workflows");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(
        subdir.join("child.ail.yaml"),
        "version: \"0.0.1\"\npipeline:\n  - id: inner\n    prompt: \"child ran\"\n",
    )
    .unwrap();

    // Parent pipeline references child via ./workflows/child.ail.yaml.
    std::fs::write(
        tmp.path().join("parent.ail.yaml"),
        "version: \"0.0.1\"\npipeline:\n  - id: call_child\n    pipeline: ./workflows/child.ail.yaml\n",
    )
    .unwrap();

    // Change CWD to the temp dir so the pipeline can be loaded as a bare filename,
    // simulating auto-discovery returning ".ail.yaml" (PathBuf::from("parent.ail.yaml")).
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Load via bare filename — the root-cause scenario.
    let pipeline = ail_core::config::load(std::path::Path::new("parent.ail.yaml")).unwrap();

    // Change CWD away so relative paths fail if not resolved against the pipeline dir.
    let unrelated = tempfile::tempdir().unwrap();
    std::env::set_current_dir(unrelated.path()).unwrap();

    let mut session = Session::new(pipeline, "hello".to_string());
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    std::env::set_current_dir(orig).unwrap();

    assert!(
        result.is_ok(),
        "Bare-filename pipeline should resolve sub-pipeline paths correctly, got: {result:?}"
    );
    assert_eq!(session.turn_log.entries().len(), 1);
    assert_eq!(session.turn_log.entries()[0].step_id, "call_child");
}

/// SPEC §11 — `on_result: pipeline:` appends the sub-pipeline result under the derived
/// step ID `<parent_id>__on_result`, not under the parent step's ID. This ensures
/// `{{ step.<id>.response }}` resolves to the parent's own response and
/// `{{ step.<id>__on_result.response }}` resolves to the sub-pipeline's response.
#[test]
fn on_result_pipeline_uses_derived_step_id_in_turn_log() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
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
            action: ResultAction::Pipeline {
                path: child_path.to_str().unwrap().to_string(),
                prompt: None,
            },
        }]),
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
    };
    let mut session = make_session(vec![trigger]);
    let runner = StubRunner::new("parent response");
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert!(
        entries.len() >= 2,
        "Expected at least 2 entries (trigger + on_result sub-pipeline), got {}",
        entries.len()
    );

    // First entry: the parent step — ID must match the declared step ID exactly.
    assert_eq!(
        entries[0].step_id, "trigger",
        "parent step should use declared step ID"
    );

    // Second entry: the on_result sub-pipeline — ID must use the derived form.
    assert_eq!(
        entries[1].step_id, "trigger__on_result",
        "on_result sub-pipeline entry must use '<id>__on_result' derived step ID (SPEC §11)"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// `prompt:` field on `on_result: pipeline:` branches parses from YAML correctly.
#[test]
fn pipeline_action_with_prompt_override_parses_from_yaml() {
    use ail_core::config::load;

    let tmp = tempfile::tempdir().unwrap();
    let child_path = fixtures_dir().join("sub_pipeline_child.ail.yaml");
    let yaml = format!(
        "version: \"0.0.1\"\npipeline:\n  - id: router\n    prompt: \"classify\"\n    on_result:\n      - always: true\n        action: \"pipeline: {}\"\n        prompt: \"{{{{ step.invocation.prompt }}}}\"\n",
        child_path.display()
    );
    let yaml_path = tmp.path().join("test_prompt.ail.yaml");
    std::fs::write(&yaml_path, yaml).unwrap();

    let pipeline = load(&yaml_path).unwrap();
    let branches = pipeline.steps[0].on_result.as_ref().unwrap();
    assert!(matches!(
        &branches[0].action,
        ResultAction::Pipeline {
            path: _,
            prompt: Some(_)
        }
    ));
}
