#![allow(clippy::result_large_err)]

use std::time::SystemTime;

use crate::config::domain::StepBody;
use crate::error::{error_types, AilError};
use crate::runner::Runner;
use crate::session::{Session, TurnEntry};
use crate::template;

/// Execute all steps in `session.pipeline` in order.
///
/// SPEC §4.2 core invariant: once execution begins, all steps run in order.
/// Early exit only via explicit declared outcomes — never silent failures.
pub fn execute(session: &mut Session, runner: &dyn Runner) -> Result<(), AilError> {
    if session.pipeline.steps.is_empty() {
        tracing::info!(run_id = %session.run_id, "empty pipeline — no steps to execute");
        return Ok(());
    }

    for step in &session.pipeline.steps {
        let step_id = step.id.as_str().to_string();

        tracing::info!(run_id = %session.run_id, step_id = %step_id, "executing step");

        match &step.body {
            StepBody::Prompt(template_text) => {
                let resolved = template::resolve(template_text, session).map_err(|mut e| {
                    e.context = Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id.clone()),
                        source: None,
                    });
                    e
                })?;

                // Resume the conversation from the last invocation so Claude
                // has the conversation history (e.g. for "Review the above output").
                let resume_id = session
                    .turn_log
                    .last_runner_session_id()
                    .map(|s| s.to_string());

                // Record intent before calling the runner. If the runner crashes
                // or hangs, this is the only evidence the step was attempted.
                session.turn_log.record_step_started(&step_id, &resolved);

                let result = runner
                    .invoke(&resolved, resume_id.as_deref())
                    .map_err(|mut e| {
                        e.context = Some(crate::error::ErrorContext {
                            pipeline_run_id: Some(session.run_id.clone()),
                            step_id: Some(step_id.clone()),
                            source: None,
                        });
                        e
                    })?;

                tracing::info!(
                    run_id = %session.run_id,
                    step_id = %step_id,
                    cost_usd = ?result.cost_usd,
                    "step complete"
                );

                session.turn_log.append(TurnEntry {
                    step_id: step_id.clone(),
                    prompt: resolved,
                    response: Some(result.response),
                    timestamp: SystemTime::now(),
                    cost_usd: result.cost_usd,
                    runner_session_id: result.session_id,
                });
            }

            StepBody::Action(crate::config::domain::ActionKind::PauseForHuman) => {
                tracing::info!(run_id = %session.run_id, step_id = %step_id, "pause_for_human");
                // v0.0.1: pause_for_human is a no-op in headless/--once mode.
                // A future phase adds interactive prompting.
            }

            StepBody::Skill(_) | StepBody::SubPipeline(_) => {
                return Err(AilError {
                    error_type: error_types::PIPELINE_ABORTED,
                    title: "Unsupported step type",
                    detail: format!(
                        "Step '{step_id}' uses a step type not yet implemented in v0.0.1"
                    ),
                    context: Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id),
                        source: None,
                    }),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{Pipeline, Step, StepBody, StepId};
    use crate::runner::stub::StubRunner;
    use crate::session::Session;

    fn make_session(steps: Vec<Step>) -> Session {
        let pipeline = Pipeline {
            steps,
            source: None,
        };
        Session::new(pipeline, "invocation prompt".to_string())
    }

    fn prompt_step(id: &str, text: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt(text.to_string()),
        }
    }

    #[test]
    fn passthrough_pipeline_runs_invocation_step() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = Session::new(Pipeline::passthrough(), "hello".to_string());
        let runner = StubRunner::new("stub response");
        let result = execute(&mut session, &runner);
        assert!(result.is_ok());
        // passthrough declares invocation as step zero; executor runs it
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "invocation");

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn single_step_pipeline_appends_to_turn_log() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![prompt_step("review", "Do a review")]);
        let runner = StubRunner::new("looks good");
        execute(&mut session, &runner).unwrap();

        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "review");
        assert_eq!(
            session.turn_log.entries()[0].response.as_deref(),
            Some("looks good")
        );

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn two_step_pipeline_runs_both_steps_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![
            prompt_step("step_a", "First prompt"),
            prompt_step("step_b", "Second prompt"),
        ]);
        let runner = StubRunner::new("stub");
        execute(&mut session, &runner).unwrap();

        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].step_id, "step_a");
        assert_eq!(entries[1].step_id, "step_b");

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn template_variable_in_prompt_is_resolved_before_runner() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![prompt_step(
            "meta",
            "The run id is: {{ pipeline.run_id }}",
        )]);
        let run_id = session.run_id.clone();
        let runner = StubRunner::new("ok");
        execute(&mut session, &runner).unwrap();

        let prompt_sent = &session.turn_log.entries()[0].prompt;
        assert!(
            prompt_sent.contains(&run_id),
            "Expected run_id in resolved prompt, got: {prompt_sent}"
        );

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn unresolvable_template_aborts_pipeline_with_error() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![prompt_step("bad", "{{ totally.unknown.variable }}")]);
        let runner = StubRunner::new("never called");
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        // No entries should have been appended
        assert_eq!(session.turn_log.entries().len(), 0);

        std::env::set_current_dir(orig).unwrap();
    }
}
