use ail_core::config;
use ail_core::config::domain::{
    Condition, ConditionExpr, ConditionOp, ContextSource, Step, StepBody, StepId,
};
use ail_core::executor::execute;
use ail_core::runner::stub::StubRunner;
use ail_core::session::Session;
use ail_core::test_helpers::make_session;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn prompt_step_with_condition(id: &str, text: &str, condition: Option<Condition>) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
    }
}

fn shell_step(id: &str, cmd: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Context(ContextSource::Shell(cmd.to_string())),
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
    }
}

/// SPEC §12.1 — condition: never skips the step without error
#[test]
fn condition_never_skips_step() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Load the fixture pipeline that has one step with condition: never and one without.
    let fixture = fixtures_dir().join("condition_steps.ail.yaml");
    let pipeline = config::load(&fixture).unwrap();
    let mut session = Session::new(pipeline, "p".to_string());

    let result = execute(&mut session, &StubRunner::new("response"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    // Only the step without condition: never should appear in the turn log.
    let entries = session.turn_log.entries();
    assert_eq!(
        entries.len(),
        1,
        "Expected exactly one entry; skipped step must not appear"
    );
    assert_eq!(
        entries[0].step_id, "run_me",
        "Expected run_me in turn log, got: {}",
        entries[0].step_id
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — condition: always executes the step (default)
#[test]
fn condition_always_executes_step() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // Build a pipeline with a single step with condition: always — it must execute normally.
    let mut session = make_session(vec![prompt_step_with_condition(
        "always_step",
        "hello",
        Some(Condition::Always),
    )]);

    let result = execute(&mut session, &StubRunner::new("response"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(
        entries.len(),
        1,
        "Expected exactly one entry for the always step"
    );
    assert_eq!(entries[0].step_id, "always_step");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: == operator checks exit code
#[test]
fn condition_expression_eq_exit_code_zero_runs_step() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        shell_step("build", "exit 0"),
        prompt_step_with_condition(
            "deploy",
            "Deploy now",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.build.exit_code }}".to_string(),
                op: ConditionOp::Eq,
                rhs: "0".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("deployed"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2, "Both steps should run");
    assert_eq!(entries[0].step_id, "build");
    assert_eq!(entries[1].step_id, "deploy");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: == operator skips when not matching
#[test]
fn condition_expression_eq_exit_code_nonzero_skips_step() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        shell_step("build", "exit 1"),
        prompt_step_with_condition(
            "deploy",
            "Deploy now",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.build.exit_code }}".to_string(),
                op: ConditionOp::Eq,
                rhs: "0".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("deployed"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1, "Only shell step should run");
    assert_eq!(entries[0].step_id, "build");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: != operator
#[test]
fn condition_expression_ne_operator() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        shell_step("build", "exit 1"),
        prompt_step_with_condition(
            "report_failure",
            "Build failed",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.build.exit_code }}".to_string(),
                op: ConditionOp::Ne,
                rhs: "0".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("reported"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2, "Both steps should run (exit code != 0)");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: contains operator
#[test]
fn condition_expression_contains_operator() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        // First step produces a prompt response
        prompt_step_with_condition("review", "Review the code", None),
        // Second step runs only if review response contains "LGTM"
        prompt_step_with_condition(
            "merge",
            "Merge PR",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.review.response }}".to_string(),
                op: ConditionOp::Contains,
                rhs: "LGTM".to_string(),
            })),
        ),
    ]);

    // StubRunner returns "Code looks good, LGTM!" which contains "LGTM"
    let result = execute(&mut session, &StubRunner::new("Code looks good, LGTM!"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2, "Both steps should run");
    assert_eq!(entries[1].step_id, "merge");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: contains operator skips when not matching
#[test]
fn condition_expression_contains_no_match_skips() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        prompt_step_with_condition("review", "Review the code", None),
        prompt_step_with_condition(
            "merge",
            "Merge PR",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.review.response }}".to_string(),
                op: ConditionOp::Contains,
                rhs: "LGTM".to_string(),
            })),
        ),
    ]);

    // StubRunner returns something that does NOT contain "LGTM"
    let result = execute(&mut session, &StubRunner::new("Needs more work"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1, "Only the review step should run");
    assert_eq!(entries[0].step_id, "review");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: unresolvable template produces typed error
#[test]
fn condition_expression_unresolvable_template_aborts() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![prompt_step_with_condition(
        "guarded",
        "do stuff",
        Some(Condition::Expression(ConditionExpr {
            lhs: "{{ step.nonexistent.exit_code }}".to_string(),
            op: ConditionOp::Eq,
            rhs: "0".to_string(),
        })),
    )]);

    let result = execute(&mut session, &StubRunner::new("response"));
    assert!(result.is_err(), "Expected error for unresolvable template");
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type(),
        ail_core::error::error_types::CONDITION_INVALID,
        "Expected CONDITION_INVALID, got: {}",
        err.error_type()
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition loaded from YAML fixture
#[test]
fn condition_expression_from_yaml_fixture() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let fixture = fixtures_dir().join("condition_expression.ail.yaml");
    let pipeline = config::load(&fixture).unwrap();
    let mut session = Session::new(pipeline, "p".to_string());

    let result = execute(&mut session, &StubRunner::new("response"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    // check_code runs, on_success runs (exit_code == 0), on_failure is skipped (exit_code != 0 is false)
    assert_eq!(entries.len(), 2, "check_code + on_success should run");
    assert_eq!(entries[0].step_id, "check_code");
    assert_eq!(entries[1].step_id, "on_success");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: contains loaded from YAML fixture
#[test]
fn condition_contains_from_yaml_fixture() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let fixture = fixtures_dir().join("condition_contains.ail.yaml");
    let pipeline = config::load(&fixture).unwrap();
    let mut session = Session::new(pipeline, "p".to_string());

    let result = execute(&mut session, &StubRunner::new("response"));
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    // check_code runs, on_ok runs (stdout contains 'ok'), on_fail is skipped
    assert_eq!(entries.len(), 2, "check_code + on_ok should run");
    assert_eq!(entries[0].step_id, "check_code");
    assert_eq!(entries[1].step_id, "on_ok");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: starts_with operator
#[test]
fn condition_expression_starts_with_operator() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        prompt_step_with_condition("check", "Check status", None),
        prompt_step_with_condition(
            "deploy",
            "Deploy",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.check.response }}".to_string(),
                op: ConditionOp::StartsWith,
                rhs: "PASS".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("PASS: all tests green"));
    assert!(result.is_ok());
    assert_eq!(session.turn_log.entries().len(), 2, "Both steps should run");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.2 — expression condition: ends_with operator
#[test]
fn condition_expression_ends_with_operator() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session(vec![
        prompt_step_with_condition("check", "Check status", None),
        prompt_step_with_condition(
            "notify",
            "Notify team",
            Some(Condition::Expression(ConditionExpr {
                lhs: "{{ step.check.response }}".to_string(),
                op: ConditionOp::EndsWith,
                rhs: "DONE".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("All work DONE"));
    assert!(result.is_ok());
    assert_eq!(session.turn_log.entries().len(), 2, "Both steps should run");

    std::env::set_current_dir(orig).unwrap();
}
