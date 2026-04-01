use ail_core::config::domain::Pipeline;
use ail_core::session::{Session, TurnEntry};
use ail_core::template::resolve;
use std::time::SystemTime;

fn make_session() -> Session {
    Session::new(Pipeline::passthrough(), "original prompt".to_string())
}

fn append_response(session: &mut Session, step_id: &str, response: &str) {
    session.turn_log.append(TurnEntry {
        step_id: step_id.to_string(),
        prompt: "p".to_string(),
        response: Some(response.to_string()),
        timestamp: SystemTime::now(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        runner_session_id: None,
        stdout: None,
        stderr: None,
        exit_code: None,
    });
}

#[test]
fn template_with_no_variables_is_unchanged() {
    let session = make_session();
    assert_eq!(resolve("no vars", &session).unwrap(), "no vars");
}

#[test]
fn last_response_resolves_from_turn_log() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session();
    append_response(&mut session, "step_1", "the answer");
    let result = resolve("{{ last_response }}", &session).unwrap();
    assert_eq!(result, "the answer");

    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn named_step_response_resolves_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let mut session = make_session();
    append_response(&mut session, "review", "looks good");
    let result = resolve("{{ step.review.response }}", &session).unwrap();
    assert_eq!(result, "looks good");

    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn pipeline_run_id_resolves_to_session_value() {
    let session = make_session();
    let result = resolve("{{ pipeline.run_id }}", &session).unwrap();
    assert_eq!(result, session.run_id);
}

#[test]
fn env_var_resolves_when_set() {
    std::env::set_var("AIL_TEST_VAR_PHASE7", "hello");
    let session = make_session();
    let result = resolve("{{ env.AIL_TEST_VAR_PHASE7 }}", &session).unwrap();
    assert_eq!(result, "hello");
    std::env::remove_var("AIL_TEST_VAR_PHASE7");
}

#[test]
fn env_var_errors_when_not_set() {
    std::env::remove_var("AIL_TEST_MISSING_VAR_PHASE7");
    let session = make_session();
    let result = resolve("{{ env.AIL_TEST_MISSING_VAR_PHASE7 }}", &session);
    assert!(result.is_err());
}

#[test]
fn unknown_step_id_returns_error_not_empty_string() {
    let session = make_session();
    let result = resolve("{{ step.nonexistent.response }}", &session);
    assert!(result.is_err());
}

#[test]
fn unrecognised_syntax_returns_error_not_empty_string() {
    let session = make_session();
    let result = resolve("{{ totally.made.up }}", &session);
    assert!(result.is_err());
}
