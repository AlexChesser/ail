use ail_core::config;
use ail_core::config::domain::{Condition, Pipeline, Step, StepBody, StepId};
use ail_core::executor::execute;
use ail_core::runner::stub::StubRunner;
use ail_core::session::Session;
use std::path::PathBuf;

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
    }
}

/// SPEC §12.1 — condition: never skips the step without error
#[test]
fn condition_never_skips_step() {
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
