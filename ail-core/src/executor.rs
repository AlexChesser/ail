#![allow(clippy::result_large_err)]

use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::config::domain::{ContextSource, ExitCodeMatch, ResultAction, ResultMatcher, StepBody};
use crate::error::{error_types, AilError};
use crate::runner::{InvokeOptions, Runner};
use crate::session::{Session, TurnEntry};
use crate::template;

/// Returned by `execute()` to distinguish successful completion variants.
#[derive(Debug)]
pub enum ExecuteOutcome {
    /// All steps ran to completion.
    Completed,
    /// A `break` action fired; remaining steps were skipped. This is not an error.
    Break { step_id: String },
}

/// Execute all steps in `session.pipeline` in order.
///
/// SPEC §4.2 core invariant: once execution begins, all steps run in order.
/// Early exit only via explicit declared outcomes — never silent failures.
pub fn execute(session: &mut Session, runner: &dyn Runner) -> Result<ExecuteOutcome, AilError> {
    if session.pipeline.steps.is_empty() {
        tracing::info!(run_id = %session.run_id, "empty pipeline — no steps to execute");
        return Ok(ExecuteOutcome::Completed);
    }

    for step in &session.pipeline.steps {
        let step_id = step.id.as_str().to_string();

        tracing::info!(run_id = %session.run_id, step_id = %step_id, "executing step");

        let entry = match &step.body {
            StepBody::Prompt(template_text) => {
                // Resolve file path if the prompt is a path reference.
                let template_text = resolve_prompt_file(template_text, &step_id)?;
                let resolved = template::resolve(&template_text, session).map_err(|mut e| {
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

                let options = InvokeOptions {
                    resume_session_id: resume_id,
                    allowed_tools: step
                        .tools
                        .as_ref()
                        .map(|t| t.allow.clone())
                        .unwrap_or_default(),
                    denied_tools: step
                        .tools
                        .as_ref()
                        .map(|t| t.deny.clone())
                        .unwrap_or_default(),
                };

                let result = runner.invoke(&resolved, options).map_err(|mut e| {
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

                TurnEntry {
                    step_id: step_id.clone(),
                    prompt: resolved,
                    response: Some(result.response),
                    timestamp: SystemTime::now(),
                    cost_usd: result.cost_usd,
                    runner_session_id: result.session_id,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                }
            }

            StepBody::Context(ContextSource::Shell(cmd)) => {
                session.turn_log.record_step_started(&step_id, cmd);

                let child = Command::new("/bin/sh")
                    .args(["-c", cmd])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| AilError {
                        error_type: error_types::RUNNER_INVOCATION_FAILED,
                        title: "Failed to spawn shell command",
                        detail: format!("Could not run shell command for step '{step_id}': {e}"),
                        context: Some(crate::error::ErrorContext {
                            pipeline_run_id: Some(session.run_id.clone()),
                            step_id: Some(step_id.clone()),
                            source: None,
                        }),
                    })?;

                let output = child.wait_with_output().map_err(|e| AilError {
                    error_type: error_types::RUNNER_INVOCATION_FAILED,
                    title: "Failed to wait for shell command",
                    detail: format!("Step '{step_id}': {e}"),
                    context: Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id.clone()),
                        source: None,
                    }),
                })?;

                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                tracing::info!(
                    run_id = %session.run_id,
                    step_id = %step_id,
                    exit_code,
                    "context shell step complete"
                );

                TurnEntry {
                    step_id: step_id.clone(),
                    prompt: cmd.clone(),
                    response: None,
                    timestamp: SystemTime::now(),
                    cost_usd: None,
                    runner_session_id: None,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    exit_code: Some(exit_code),
                }
            }

            StepBody::Action(crate::config::domain::ActionKind::PauseForHuman) => {
                tracing::info!(run_id = %session.run_id, step_id = %step_id, "pause_for_human");
                // v0.1: pause_for_human is a no-op in headless/--once mode.
                continue;
            }

            StepBody::Skill(_) | StepBody::SubPipeline(_) => {
                return Err(AilError {
                    error_type: error_types::PIPELINE_ABORTED,
                    title: "Unsupported step type",
                    detail: format!(
                        "Step '{step_id}' uses a step type not yet implemented in v0.1"
                    ),
                    context: Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id),
                        source: None,
                    }),
                });
            }
        };

        session.turn_log.append(entry);

        // Evaluate on_result branches after every step that produced an entry.
        if let Some(branches) = &step.on_result {
            let last_entry = session.turn_log.entries().last().expect("just appended");
            if let Some(action) = evaluate_on_result(branches, last_entry) {
                match action {
                    ResultAction::Continue => {}
                    ResultAction::Break => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result break — stopping pipeline early"
                        );
                        return Ok(ExecuteOutcome::Break {
                            step_id: step_id.clone(),
                        });
                    }
                    ResultAction::AbortPipeline => {
                        return Err(AilError {
                            error_type: error_types::PIPELINE_ABORTED,
                            title: "Pipeline aborted by on_result",
                            detail: format!("Step '{step_id}' on_result fired abort_pipeline"),
                            context: Some(crate::error::ErrorContext {
                                pipeline_run_id: Some(session.run_id.clone()),
                                step_id: Some(step_id),
                                source: None,
                            }),
                        });
                    }
                    ResultAction::PauseForHuman => {
                        // v0.1: pause_for_human in on_result is a no-op in headless mode.
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result pause_for_human (no-op in headless mode)"
                        );
                    }
                }
            }
        }
    }

    Ok(ExecuteOutcome::Completed)
}

/// Evaluate `on_result` branches against the most recent `TurnEntry`.
/// Returns the action of the first matching branch, or `None` if no branch matches.
fn evaluate_on_result(
    branches: &[crate::config::domain::ResultBranch],
    entry: &TurnEntry,
) -> Option<ResultAction> {
    for branch in branches {
        let matched = match &branch.matcher {
            ResultMatcher::Contains(text) => {
                let haystack = entry
                    .response
                    .as_deref()
                    .or(entry.stdout.as_deref())
                    .unwrap_or("");
                let haystack_lower = haystack.to_lowercase();
                haystack_lower.contains(&text.to_lowercase())
            }
            ResultMatcher::ExitCode(ExitCodeMatch::Exact(n)) => entry.exit_code == Some(*n),
            ResultMatcher::ExitCode(ExitCodeMatch::Any) => {
                // `any` matches any non-zero exit code — does NOT match 0.
                matches!(entry.exit_code, Some(c) if c != 0)
            }
            ResultMatcher::Always => true,
        };

        if matched {
            return Some(match branch.action {
                ResultAction::Continue => ResultAction::Continue,
                ResultAction::Break => ResultAction::Break,
                ResultAction::AbortPipeline => ResultAction::AbortPipeline,
                ResultAction::PauseForHuman => ResultAction::PauseForHuman,
            });
        }
    }
    None
}

/// If `prompt_text` starts with a path prefix (`./`, `../`, `~/`, `/`), read the file
/// at that path and return its contents as the prompt template. Otherwise returns the
/// original string unchanged. `~/` is expanded using the user's home directory.
fn resolve_prompt_file(prompt_text: &str, step_id: &str) -> Result<String, AilError> {
    let is_path = prompt_text.starts_with("./")
        || prompt_text.starts_with("../")
        || prompt_text.starts_with("~/")
        || prompt_text.starts_with('/');

    if !is_path {
        return Ok(prompt_text.to_string());
    }

    let path = if let Some(rel) = prompt_text.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| AilError {
            error_type: error_types::CONFIG_FILE_NOT_FOUND,
            title: "Cannot resolve home directory",
            detail: format!(
                "Step '{step_id}' prompt path starts with ~/ but home dir is unavailable"
            ),
            context: None,
        })?;
        home.join(rel)
    } else {
        std::path::PathBuf::from(prompt_text)
    };

    std::fs::read_to_string(&path).map_err(|e| AilError {
        error_type: error_types::CONFIG_FILE_NOT_FOUND,
        title: "Prompt file not found",
        detail: format!(
            "Step '{step_id}' prompt file '{}' could not be read: {e}",
            path.display()
        ),
        context: None,
    })
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
            tools: None,
            on_result: None,
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
