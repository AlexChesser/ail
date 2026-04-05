use ail_core::config::domain::{
    ExitCodeMatch, Pipeline, ResultAction, ResultBranch, ResultMatcher, Step, StepBody, StepId,
};
use ail_core::executor::{
    execute, execute_with_control, ExecuteOutcome, ExecutionControl, ExecutorEvent,
};
use ail_core::runner::stub::StubRunner;
use ail_core::session::Session;
use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::Arc;

fn make_session(steps: Vec<Step>) -> Session {
    Session::new(
        Pipeline {
            steps,
            defaults: Default::default(),
            source: None,
            default_tools: None,
        },
        "prompt".to_string(),
    )
}

fn prompt_step_with_on_result(
    id: &str,
    response: &str,
    branches: Vec<ResultBranch>,
) -> (Step, StubRunner) {
    let step = Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt("test".to_string()),
        message: None,
        tools: None,
        model: None,
        on_result: Some(branches),
        runner: None,
    };
    (step, StubRunner::new(response))
}

fn context_step_with_exit(id: &str, exit_code: i32, branches: Vec<ResultBranch>) -> Step {
    // We use a shell command whose exit reflects the desired code.
    let cmd = if exit_code == 0 {
        "true".to_string()
    } else {
        format!("exit {}", exit_code)
    };
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Context(ail_core::config::domain::ContextSource::Shell(cmd)),
        message: None,
        tools: None,
        model: None,
        on_result: Some(branches),
        runner: None,
    }
}

fn prompt_step(id: &str, text: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
        message: None,
        tools: None,
        model: None,
        on_result: None,
        runner: None,
    }
}

/// SPEC §5.4 — on_result contains match: pipeline continues to next step
#[test]
fn on_result_contains_match_continues() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let (step1, runner) = prompt_step_with_on_result(
        "check",
        "the output contains success message",
        vec![ResultBranch {
            matcher: ResultMatcher::Contains("success".to_string()),
            action: ResultAction::Continue,
        }],
    );
    let step2 = prompt_step("next", "next step");
    let mut session = make_session(vec![step1, step2]);
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    // Both steps should have run
    assert_eq!(session.turn_log.entries().len(), 2);

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — on_result abort_pipeline exits as AilError
#[test]
fn on_result_abort_pipeline_exits_as_ail_error() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "linter",
        1,
        vec![ResultBranch {
            matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
            action: ResultAction::AbortPipeline,
        }],
    );
    let mut session = make_session(vec![step]);
    let result = execute(&mut session, &StubRunner::new("x"));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type,
        ail_core::error::error_types::PIPELINE_ABORTED
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — on_result break exits as Ok(Break), not Err
#[test]
fn on_result_break_exits_as_ok_not_err() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step1 = context_step_with_exit(
        "tests",
        0,
        vec![ResultBranch {
            matcher: ResultMatcher::ExitCode(ExitCodeMatch::Exact(0)),
            action: ResultAction::Break,
        }],
    );
    let step2 = prompt_step("unreachable", "this should not run");
    let mut session = make_session(vec![step1, step2]);
    let result = execute(&mut session, &StubRunner::new("x"));

    assert!(result.is_ok());
    match result.unwrap() {
        ExecuteOutcome::Break { step_id } => assert_eq!(step_id, "tests"),
        ExecuteOutcome::Completed => panic!("expected Break, got Completed"),
        ExecuteOutcome::Error(e) => panic!("expected Break, got Error: {}", e),
    }
    // step2 must NOT have run
    assert_eq!(session.turn_log.entries().len(), 1);

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — pause_for_human in on_result is a no-op in the uncontrolled executor (no HITL channel)
#[test]
fn on_result_pause_for_human_suspends() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "gate",
        0,
        vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: ResultAction::PauseForHuman,
        }],
    );
    let mut session = make_session(vec![step]);
    let result = execute(&mut session, &StubRunner::new("x"));
    // uncontrolled executor: pause_for_human is a no-op — pipeline continues
    assert!(result.is_ok());

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4, §13 — pause_for_human in on_result blocks in the controlled executor until a
/// hitl_response is received, then resumes and completes the pipeline.
#[test]
fn on_result_pause_for_human_blocks_in_controlled_executor() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "gate",
        0,
        vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: ResultAction::PauseForHuman,
        }],
    );
    let mut session = make_session(vec![step]);

    let (event_tx, event_rx) = mpsc::channel::<ExecutorEvent>();
    let (hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let control = ExecutionControl {
        pause_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        kill_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        permission_responder: None,
    };
    let disabled_steps = HashSet::new();

    // Spawn a thread that waits for the HitlGateReached event, then unblocks the gate.
    let gate_thread = std::thread::spawn(move || {
        for ev in event_rx {
            if matches!(ev, ExecutorEvent::HitlGateReached { .. }) {
                let _ = hitl_tx.send("approved".to_string());
                break;
            }
        }
    });

    let result = execute_with_control(
        &mut session,
        &StubRunner::new("x"),
        &control,
        &disabled_steps,
        event_tx,
        hitl_rx,
    );

    let _ = gate_thread.join();
    assert!(result.is_ok());

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — exit_code: 0 branch matches zero exit, continues
#[test]
fn on_result_exit_code_0_continue() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "lint",
        0,
        vec![ResultBranch {
            matcher: ResultMatcher::ExitCode(ExitCodeMatch::Exact(0)),
            action: ResultAction::Continue,
        }],
    );
    let mut session = make_session(vec![step]);
    let result = execute(&mut session, &StubRunner::new("x"));
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — exit_code: any matches non-zero exit codes
#[test]
fn on_result_exit_code_any_matches_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "lint",
        1,
        vec![ResultBranch {
            matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
            action: ResultAction::Continue,
        }],
    );
    let step2 = prompt_step("next", "next");
    let mut session = make_session(vec![step, step2]);
    let result = execute(&mut session, &StubRunner::new("x"));
    // any matched, action was continue, pipeline runs next step
    assert!(result.is_ok());
    assert_eq!(session.turn_log.entries().len(), 2);

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — exit_code: any does NOT match exit code 0
#[test]
fn on_result_exit_code_any_does_not_match_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "lint",
        0,
        vec![
            // `any` would abort if it matched; but it shouldn't match exit 0
            ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
                action: ResultAction::AbortPipeline,
            },
            // fallthrough
            ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Continue,
            },
        ],
    );
    let mut session = make_session(vec![step]);
    let result = execute(&mut session, &StubRunner::new("x"));
    // Should complete, not abort
    assert!(result.is_ok());

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — first matching branch wins, subsequent branches are not evaluated
#[test]
fn on_result_first_match_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "lint",
        0,
        vec![
            // First branch matches and continues
            ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Exact(0)),
                action: ResultAction::Continue,
            },
            // Second branch also matches but should NOT fire
            ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::AbortPipeline,
            },
        ],
    );
    let mut session = make_session(vec![step]);
    let result = execute(&mut session, &StubRunner::new("x"));
    assert!(result.is_ok(), "First branch (continue) should have won");

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — `always:` branch fires unconditionally
#[test]
fn on_result_always_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step = context_step_with_exit(
        "tests",
        0,
        vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: ResultAction::Break,
        }],
    );
    let step2 = prompt_step("unreachable", "never");
    let mut session = make_session(vec![step, step2]);
    let result = execute(&mut session, &StubRunner::new("x"));
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ExecuteOutcome::Break { .. }));
    assert_eq!(session.turn_log.entries().len(), 1);

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §5.4 — break skips all remaining steps
#[test]
fn on_result_break_skips_remaining_steps() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let step1 = context_step_with_exit(
        "tests",
        0,
        vec![ResultBranch {
            matcher: ResultMatcher::ExitCode(ExitCodeMatch::Exact(0)),
            action: ResultAction::Break,
        }],
    );
    let step2 = prompt_step("fix", "fix the code");
    let step3 = prompt_step("verify", "verify the fix");
    let mut session = make_session(vec![step1, step2, step3]);
    let result = execute(&mut session, &StubRunner::new("x"));

    assert!(result.is_ok());
    match result.unwrap() {
        ExecuteOutcome::Break { step_id } => assert_eq!(step_id, "tests"),
        _ => panic!("expected Break"),
    }
    // Only step1 should have run
    assert_eq!(session.turn_log.entries().len(), 1);
    assert_eq!(session.turn_log.entries()[0].step_id, "tests");

    std::env::set_current_dir(orig).unwrap();
}
