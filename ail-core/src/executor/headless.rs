//! Headless (non-interactive) pipeline execution.
//!
//! # Why the tests live here
//!
//! `executor/core.rs` is the shared execution kernel; it is tested indirectly
//! through the public `execute` function in this file. Keeping the integration
//! tests here (rather than in `core.rs`) keeps the kernel file focused on
//! the dispatch logic and avoids a 1000-line file. All tests that exercise
//! the step-dispatch loop and `on_result` branching live in this file.

#![allow(clippy::result_large_err)]

use crate::error::AilError;
use crate::runner::Runner;
use crate::session::Session;

use super::core::{execute_core, NullObserver};
use super::events::ExecuteOutcome;

/// Execute all steps in `session.pipeline` in order (headless mode).
///
/// SPEC §4.2 core invariant: once execution begins, all steps run in order.
/// Early exit only via explicit declared outcomes — never silent failures.
pub fn execute(session: &mut Session, runner: &dyn Runner) -> Result<ExecuteOutcome, AilError> {
    execute_core(session, runner, &mut NullObserver, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{
        ActionKind, Condition, ContextSource, ExitCodeMatch, Pipeline, ResultAction, ResultBranch,
        ResultMatcher, Step, StepBody, StepId,
    };
    use crate::error::error_types;
    use crate::runner::stub::StubRunner;
    use crate::session::{NullProvider, Session};
    use crate::test_helpers::{make_session, prompt_step};

    fn prompt_step_with_on_result(id: &str, branches: Vec<ResultBranch>) -> Step {
        Step {
            on_result: Some(branches),
            ..prompt_step(id, "test prompt")
        }
    }

    fn context_shell_step(id: &str, cmd: &str) -> Step {
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
        }
    }

    fn context_shell_step_with_on_result(id: &str, cmd: &str, branches: Vec<ResultBranch>) -> Step {
        Step {
            on_result: Some(branches),
            ..context_shell_step(id, cmd)
        }
    }

    fn action_step(id: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Action(ActionKind::PauseForHuman),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    fn skill_step(id: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Skill(std::path::PathBuf::from("some-skill")),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    /// SPEC §12 — condition: never causes the step to be skipped entirely.
    #[test]
    fn condition_never_skips_step() {
        let mut skipped = prompt_step("skipped", "this should not run");
        skipped.condition = Some(Condition::Never);

        let after = prompt_step("after", "this should run");

        let mut session = make_session(vec![skipped, after]);
        let runner = StubRunner::new("ok");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1, "only the 'after' step should have run");
        assert_eq!(entries[0].step_id, "after");
    }

    /// SPEC §12 — condition: never skips all steps when every step is never.
    #[test]
    fn condition_never_on_all_steps_produces_empty_turn_log() {
        let mut s1 = prompt_step("s1", "text");
        s1.condition = Some(Condition::Never);
        let mut s2 = prompt_step("s2", "text");
        s2.condition = Some(Condition::Never);

        let mut session = make_session(vec![s1, s2]);
        let runner = StubRunner::new("never called");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            session.turn_log.entries().is_empty(),
            "no entries expected when all steps are skipped"
        );
    }

    /// SPEC §5.3 — context:shell: step populates stdout, stderr, and exit_code in turn log.
    #[test]
    fn context_shell_step_populates_stdout_stderr_exit_code() {
        let step = context_shell_step("ctx", "echo hello_out; echo err_out >&2; exit 0");
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.step_id, "ctx");
        assert!(
            entry.stdout.as_deref().unwrap_or("").contains("hello_out"),
            "stdout should contain 'hello_out'"
        );
        assert_eq!(entry.exit_code, Some(0));
        assert!(
            entry.response.is_none(),
            "context steps have no response field"
        );
    }

    /// SPEC §5.3 — context:shell: step with non-zero exit code captures exit_code correctly.
    #[test]
    fn context_shell_step_captures_nonzero_exit_code() {
        let step = context_shell_step("ctx", "exit 42");
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(
            result.is_ok(),
            "non-zero exit code is a result, not an error"
        );
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].exit_code, Some(42));
    }

    /// SPEC §5.4 — on_result contains: matcher that matches → action fires (continue).
    #[test]
    fn on_result_contains_match_action_fires() {
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Contains("needle".to_string()),
                action: ResultAction::Continue,
            }],
        );
        let after = prompt_step("after", "next");
        let mut session = make_session(vec![step, after]);
        // Runner returns a response containing the needle.
        let runner = StubRunner::new("response with needle in it");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 2, "both steps should have run");
    }

    /// SPEC §5.4 — on_result contains: matcher that doesn't match → no action fires, pipeline continues.
    #[test]
    fn on_result_contains_no_match_no_action_fires() {
        // Branch: if response contains "needle" → break. But response won't contain needle.
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Contains("needle".to_string()),
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("after", "next");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("no match here");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(
            entries.len(),
            2,
            "both steps should run because contains: did not match"
        );
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Completed),
            "should complete, not break"
        );
    }

    /// SPEC §5.4 — on_result contains: matching is case-insensitive.
    #[test]
    fn on_result_contains_match_is_case_insensitive() {
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Contains("SUCCESS".to_string()),
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("unreachable", "never");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("task ended with success status");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Break { .. }),
            "case-insensitive match should fire break"
        );
        assert_eq!(session.turn_log.entries().len(), 1);
    }

    /// SPEC §5.4 — on_result: break returns Ok(Break { step_id }) with the correct step id.
    #[test]
    fn on_result_break_returns_correct_step_id() {
        let step = prompt_step_with_on_result(
            "breaking_step",
            vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("unreachable", "never");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("any response");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        match result.unwrap() {
            ExecuteOutcome::Break { step_id } => {
                assert_eq!(step_id, "breaking_step");
            }
            other => panic!("expected Break, got {other:?}"),
        }
        assert_eq!(session.turn_log.entries().len(), 1);
    }

    /// SPEC §5.4 — on_result: abort_pipeline returns Err with PIPELINE_ABORTED error_type.
    #[test]
    fn on_result_abort_pipeline_returns_pipeline_aborted_error() {
        let step = prompt_step_with_on_result(
            "aborter",
            vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::AbortPipeline,
            }],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("any response");
        let result = execute(&mut session, &runner);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::PIPELINE_ABORTED,
            "on_result abort_pipeline must return PIPELINE_ABORTED"
        );
        assert!(
            err.detail().contains("aborter"),
            "error detail should reference the step id"
        );
    }

    /// SPEC §5.4 — on_result: exit_code matcher on context:shell: step.
    #[test]
    fn on_result_exit_code_exact_match_on_context_step() {
        let step = context_shell_step_with_on_result(
            "linter",
            "exit 2",
            vec![ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Exact(2)),
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("unreachable", "never");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Break { .. }),
            "exit_code: 2 should trigger break"
        );
        assert_eq!(session.turn_log.entries().len(), 1);
    }

    /// SPEC §5.4 — on_result: exit_code: any matches non-zero on context step, fires action.
    #[test]
    fn on_result_exit_code_any_matches_nonzero_context_step() {
        let step = context_shell_step_with_on_result(
            "build",
            "exit 1",
            vec![ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
                action: ResultAction::AbortPipeline,
            }],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().error_type(),
            error_types::PIPELINE_ABORTED
        );
    }

    /// SPEC §4.2 — pause_for_human action step is a no-op in headless mode; pipeline continues.
    #[test]
    fn pause_for_human_action_step_is_noop_in_headless_mode() {
        let pause = action_step("gate");
        let after = prompt_step("after", "continue");
        let mut session = make_session(vec![pause, after]);
        let runner = StubRunner::new("ok");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Completed),
            "pipeline should complete after no-op pause_for_human"
        );
        // pause_for_human continues without appending a TurnEntry; only 'after' adds an entry.
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1, "pause_for_human produces no turn entry");
        assert_eq!(entries[0].step_id, "after");
    }

    /// SPEC §4.2 — skill: step aborts with PIPELINE_ABORTED (stub in v0.2).
    #[test]
    fn skill_step_aborts_with_pipeline_aborted_error() {
        let step = skill_step("my_skill");
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::PIPELINE_ABORTED,
            "skill: step must abort with PIPELINE_ABORTED until implemented"
        );
    }

    /// SPEC §4.2 — empty pipeline (no steps) returns Completed without any entries.
    #[test]
    fn empty_pipeline_returns_completed() {
        let mut session = make_session(vec![]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Completed),
            "empty pipeline should return Completed"
        );
        assert!(session.turn_log.entries().is_empty());
    }

    /// SPEC §4.2 — multi-step pipeline: all steps run in order and produce entries.
    #[test]
    fn multi_step_pipeline_runs_all_steps_in_order() {
        let s1 = prompt_step("first", "prompt one");
        let s2 = prompt_step("second", "prompt two");
        let s3 = prompt_step("third", "prompt three");
        let mut session = make_session(vec![s1, s2, s3]);
        let runner = StubRunner::new("stub");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].step_id, "first");
        assert_eq!(entries[1].step_id, "second");
        assert_eq!(entries[2].step_id, "third");
    }

    /// Passthrough pipeline (Pipeline::passthrough) runs the invocation step.
    #[test]
    fn passthrough_pipeline_runs_invocation_step() {
        let mut session = Session::new(Pipeline::passthrough(), "hello world".to_string())
            .with_log_provider(Box::new(NullProvider));
        let runner = StubRunner::new("response");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].step_id, "invocation");
    }

    /// SPEC §5.4 — no on_result defined: pipeline always continues to next step.
    #[test]
    fn step_without_on_result_always_continues() {
        let s1 = prompt_step("s1", "step one");
        let s2 = prompt_step("s2", "step two");
        let mut session = make_session(vec![s1, s2]);
        let runner = StubRunner::new("anything");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 2);
    }

    /// SPEC §5.4 — on_result: continue branch fires and pipeline continues to next step.
    #[test]
    fn on_result_continue_branch_pipeline_continues() {
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Continue,
            }],
        );
        let after = prompt_step("after", "next");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("anything");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
        assert_eq!(session.turn_log.entries().len(), 2);
    }
}
