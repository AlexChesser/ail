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

/// SPEC §12.2/§12.3 — `matches` operator with basic regex, unanchored and
/// case-sensitive by default. The RegexCondition is hand-built here (parser
/// coverage lives in the regex_literal unit tests); this exercises the
/// evaluator end-to-end.
#[test]
fn condition_matches_operator_basic() {
    use ail_core::config::domain::RegexCondition;

    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let regex = regex::Regex::new(r"error|warning").unwrap();
    let mut session = make_session(vec![
        prompt_step_with_condition("lint", "Run lint", None),
        prompt_step_with_condition(
            "triage",
            "Triage issues",
            Some(Condition::Regex(RegexCondition {
                lhs: "{{ step.lint.response }}".to_string(),
                regex,
                source: "/error|warning/".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("3 warnings, 0 errors"));
    assert!(result.is_ok());
    assert_eq!(
        session.turn_log.entries().len(),
        2,
        "triage should run when lint output matches"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.3 — `matches` operator is case-sensitive by default; the `i`
/// flag must be set explicitly to toggle case-insensitive matching.
#[test]
fn condition_matches_operator_case_sensitive_by_default() {
    use ail_core::config::domain::RegexCondition;

    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    // No `i` flag — pattern `error` should NOT match "ERROR".
    let regex = regex::Regex::new(r"error").unwrap();
    let mut session = make_session(vec![
        prompt_step_with_condition("check", "Check", None),
        prompt_step_with_condition(
            "act",
            "Act on error",
            Some(Condition::Regex(RegexCondition {
                lhs: "{{ step.check.response }}".to_string(),
                regex,
                source: "/error/".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("FATAL ERROR: boom"));
    assert!(result.is_ok());
    assert_eq!(
        session.turn_log.entries().len(),
        1,
        "act step should be skipped — case-sensitive pattern does not match uppercase"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.3 — `matches` with `i` flag enables case-insensitive matching.
#[test]
fn condition_matches_operator_i_flag() {
    use ail_core::config::domain::RegexCondition;

    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let regex = regex::RegexBuilder::new("error")
        .case_insensitive(true)
        .build()
        .unwrap();
    let mut session = make_session(vec![
        prompt_step_with_condition("check", "Check", None),
        prompt_step_with_condition(
            "act",
            "Act on error",
            Some(Condition::Regex(RegexCondition {
                lhs: "{{ step.check.response }}".to_string(),
                regex,
                source: "/error/i".to_string(),
            })),
        ),
    ]);

    let result = execute(&mut session, &StubRunner::new("FATAL ERROR: boom"));
    assert!(result.is_ok());
    assert_eq!(
        session.turn_log.entries().len(),
        2,
        "case-insensitive match should succeed"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.3 — `matches` parsing via the condition expression grammar.
/// Covers the full path: raw YAML expression → condition parser →
/// RegexCondition → evaluator.
#[test]
fn condition_matches_operator_via_parser() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let yaml = r#"
version: "1"
pipeline:
  - id: lint
    prompt: "run lint"
  - id: triage
    prompt: "triage"
    condition: "{{ step.lint.response }} matches /ERROR|FAIL/i"
"#;
    let pipeline_path = tmp.path().join(".ail.yaml");
    std::fs::write(&pipeline_path, yaml).unwrap();
    let pipeline = ail_core::config::load(&pipeline_path).unwrap();
    let mut session = Session::new(pipeline, "p".to_string());

    let result = execute(
        &mut session,
        &StubRunner::new("Process failed with FATAL error"),
    );
    assert!(result.is_ok());
    assert_eq!(
        session.turn_log.entries().len(),
        2,
        "triage should run when matches fires (case-insensitive)"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.3 — invalid regex at parse time fails pipeline load.
#[test]
fn condition_matches_invalid_regex_fails_at_parse() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let yaml = r#"
version: "1"
pipeline:
  - id: broken
    prompt: "x"
    condition: "{{ step.x.response }} matches /[unclosed/"
"#;
    let pipeline_path = tmp.path().join(".ail.yaml");
    std::fs::write(&pipeline_path, yaml).unwrap();
    let err = ail_core::config::load(&pipeline_path).unwrap_err();
    assert_eq!(
        err.error_type(),
        ail_core::error::error_types::CONFIG_VALIDATION_FAILED
    );
    assert!(
        err.detail().contains("failed to compile"),
        "detail should explain regex compile failure: {}",
        err.detail()
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §12.3 — `g` flag is explicitly rejected with a specific message.
#[test]
fn condition_matches_g_flag_rejected() {
    let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let yaml = r#"
version: "1"
pipeline:
  - id: broken
    prompt: "x"
    condition: "{{ step.x.response }} matches /warn/gi"
"#;
    let pipeline_path = tmp.path().join(".ail.yaml");
    std::fs::write(&pipeline_path, yaml).unwrap();
    let err = ail_core::config::load(&pipeline_path).unwrap_err();
    assert!(
        err.detail().contains("'g' flag"),
        "detail should call out the g flag specifically: {}",
        err.detail()
    );

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
