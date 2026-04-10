//! TUI-controlled pipeline execution — `execute_with_control()`.

#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use crate::config::domain::{Condition, ContextSource, ResultAction, StepBody};
use crate::error::{error_types, AilError};
use crate::runner::{InvokeOptions, Runner, RunnerEvent};
use crate::session::{Session, TurnEntry};
use crate::template;

use super::events::{ExecuteOutcome, ExecutionControl, ExecutorEvent};
use super::headless::execute_sub_pipeline;
use super::helpers::{
    build_step_runner_box, build_tool_policy, evaluate_on_result, resolve_prompt_file,
    resolve_step_provider, resolve_step_system_prompts, run_shell_command,
};

/// Execute the pipeline with live control signals and event streaming for the TUI.
///
/// Additive counterpart to `execute()` — the original function is unchanged.
/// Sends `ExecutorEvent`s through `event_tx`; respects kill/pause flags between steps.
/// Steps listed in `disabled_steps` are skipped with a `StepSkipped` event.
/// Blocks on `hitl_rx.recv()` when a `pause_for_human` step is reached (M10).
pub fn execute_with_control(
    session: &mut Session,
    runner: &dyn Runner,
    control: &ExecutionControl,
    disabled_steps: &HashSet<String>,
    event_tx: mpsc::Sender<ExecutorEvent>,
    hitl_rx: mpsc::Receiver<String>,
) -> Result<ExecuteOutcome, AilError> {
    let total_steps = session.pipeline.steps.len();

    if total_steps == 0 {
        tracing::info!(run_id = %session.run_id, "empty pipeline — no steps to execute");
        let _ = event_tx.send(ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed));
        return Ok(ExecuteOutcome::Completed);
    }

    // Clone to avoid borrow issues while mutating session inside the loop.
    let steps: Vec<_> = session.pipeline.steps.clone();

    for (step_index, step) in steps.iter().enumerate() {
        let step_id = step.id.as_str().to_string();

        // Check kill flag between steps.
        if control.kill_requested.load(Ordering::SeqCst) {
            tracing::info!(run_id = %session.run_id, step_id = %step_id, "kill requested — stopping pipeline");
            break;
        }

        // Check pause flag — spin-sleep until cleared or kill is set.
        while control.pause_requested.load(Ordering::SeqCst) {
            if control.kill_requested.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if control.kill_requested.load(Ordering::SeqCst) {
            break;
        }

        // Skip disabled steps.
        if disabled_steps.contains(&step_id) {
            let _ = event_tx.send(ExecutorEvent::StepSkipped {
                step_id: step_id.clone(),
            });
            continue;
        }

        // Check condition — skip the step if condition is Never (SPEC §12).
        if step.condition == Some(Condition::Never) {
            tracing::info!(run_id = %session.run_id, step_id = %step_id, "step skipped by condition: never");
            let _ = event_tx.send(ExecutorEvent::StepSkipped {
                step_id: step_id.clone(),
            });
            continue;
        }

        // Prompt steps emit StepStarted after template resolution so resolved_prompt is available.
        // All other step types emit it here with resolved_prompt: None.
        if !matches!(step.body, StepBody::Prompt(_)) {
            let _ = event_tx.send(ExecutorEvent::StepStarted {
                step_id: step_id.clone(),
                step_index,
                total_steps,
                resolved_prompt: None,
            });
        }

        tracing::info!(run_id = %session.run_id, step_id = %step_id, "executing step (controlled)");

        // Base dir for resolving ./relative file paths — the pipeline file's parent dir (SPEC §5.2).
        // Owned PathBuf so we can pass it to execute_sub_pipeline without holding a borrow on session.
        let pipeline_base_dir_buf: Option<std::path::PathBuf> = session
            .pipeline
            .source
            .as_deref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        let pipeline_base_dir = pipeline_base_dir_buf.as_deref();

        let entry = match &step.body {
            StepBody::Prompt(template_text) => {
                let template_text =
                    match resolve_prompt_file(template_text, &step_id, pipeline_base_dir) {
                        Ok(t) => t,
                        Err(e) => {
                            let _ = event_tx.send(ExecutorEvent::StepFailed {
                                step_id: step_id.clone(),
                                error: e.detail.clone(),
                            });
                            return Err(e);
                        }
                    };
                let resolved = match template::resolve(&template_text, session) {
                    Ok(r) => r,
                    Err(e) => {
                        let e = e.with_step_context(&session.run_id, &step_id);
                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                            step_id: step_id.clone(),
                            error: e.detail.clone(),
                        });
                        return Err(e);
                    }
                };

                // Emit StepStarted here for Prompt steps — resolved_prompt is now available.
                let _ = event_tx.send(ExecutorEvent::StepStarted {
                    step_id: step_id.clone(),
                    step_index,
                    total_steps,
                    resolved_prompt: Some(resolved.clone()),
                });

                let resume_id = if step.resume {
                    session
                        .turn_log
                        .last_runner_session_id()
                        .map(|s| s.to_string())
                } else {
                    None
                };
                session.turn_log.record_step_started(&step_id, &resolved);

                let (resolved_system_prompt, resolved_append_system_prompt) =
                    resolve_step_system_prompts(step, session, &step_id, pipeline_base_dir)
                        .inspect_err(|e| {
                            let _ = event_tx.send(ExecutorEvent::StepFailed {
                                step_id: step_id.clone(),
                                error: e.detail.clone(),
                            });
                        })?;

                let resolved_provider = resolve_step_provider(session, step);
                let effective_tools = step
                    .tools
                    .as_ref()
                    .or(session.pipeline.default_tools.as_ref());
                let step_runner_box = build_step_runner_box(step)?;
                let effective_runner: &dyn Runner = step_runner_box
                    .as_deref()
                    .map(|b| b as &dyn Runner)
                    .unwrap_or(runner);

                let extensions = effective_runner.build_extensions(&resolved_provider);
                let options = InvokeOptions {
                    resume_session_id: resume_id,
                    tool_policy: build_tool_policy(effective_tools),
                    model: resolved_provider.model,
                    extensions,
                    permission_responder: control.permission_responder.clone(),
                    cancel_token: Some(Arc::clone(&control.kill_requested)),
                    system_prompt: resolved_system_prompt,
                    append_system_prompt: resolved_append_system_prompt,
                };

                // Create a sub-channel for runner events.
                let (runner_tx, runner_rx) = mpsc::channel::<RunnerEvent>();

                let event_tx_clone = event_tx.clone();
                // Forward runner events to the main event channel on a separate thread.
                let fwd_handle = std::thread::spawn(move || {
                    for ev in runner_rx {
                        let _ = event_tx_clone.send(ExecutorEvent::RunnerEvent { event: ev });
                    }
                });

                let invoke_result =
                    effective_runner.invoke_streaming(&resolved, options, runner_tx);
                let _ = fwd_handle.join();

                match invoke_result {
                    Ok(result) => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            cost_usd = ?result.cost_usd,
                            "step complete (controlled)"
                        );
                        let _ = event_tx.send(ExecutorEvent::StepCompleted {
                            step_id: step_id.clone(),
                            cost_usd: result.cost_usd,
                            input_tokens: result.input_tokens,
                            output_tokens: result.output_tokens,
                            response: Some(result.response.clone()),
                            model: result.model.clone(),
                        });

                        TurnEntry::from_prompt(step_id.clone(), resolved, result)
                    }
                    Err(e) => {
                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                            step_id: step_id.clone(),
                            error: e.detail.clone(),
                        });
                        return Err(e);
                    }
                }
            }

            StepBody::Context(ContextSource::Shell(cmd)) => {
                session.turn_log.record_step_started(&step_id, cmd);
                let (stdout, stderr, exit_code) =
                    match run_shell_command(&session.run_id, &step_id, cmd) {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = event_tx.send(ExecutorEvent::StepFailed {
                                step_id: step_id.clone(),
                                error: e.detail.clone(),
                            });
                            return Err(e);
                        }
                    };
                tracing::info!(
                    run_id = %session.run_id,
                    step_id = %step_id,
                    exit_code,
                    "context shell step complete (controlled)"
                );
                let _ = event_tx.send(ExecutorEvent::StepCompleted {
                    step_id: step_id.clone(),
                    cost_usd: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    response: None,
                    model: None,
                });
                TurnEntry::from_context(step_id.clone(), cmd.clone(), stdout, stderr, exit_code)
            }

            StepBody::Action(crate::config::domain::ActionKind::PauseForHuman) => {
                tracing::info!(run_id = %session.run_id, step_id = %step_id, "pause_for_human — waiting for HITL response");
                let _ = event_tx.send(ExecutorEvent::HitlGateReached {
                    step_id: step_id.clone(),
                    message: step.message.clone(),
                });
                // Block until the TUI sends a response (or the channel is dropped).
                let _response = hitl_rx.recv().unwrap_or_default();
                tracing::info!(run_id = %session.run_id, step_id = %step_id, "HITL gate unblocked — resuming");
                let _ = event_tx.send(ExecutorEvent::StepCompleted {
                    step_id: step_id.clone(),
                    cost_usd: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    response: None,
                    model: None,
                });
                continue;
            }

            StepBody::SubPipeline {
                path: path_template,
                prompt,
            } => {
                session
                    .turn_log
                    .record_step_started(&step_id, path_template);
                match execute_sub_pipeline(
                    path_template,
                    prompt.as_deref(),
                    &step_id,
                    session,
                    runner,
                    1,
                    pipeline_base_dir,
                ) {
                    Ok(entry) => {
                        let _ = event_tx.send(ExecutorEvent::StepCompleted {
                            step_id: step_id.clone(),
                            cost_usd: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            response: None,
                            model: None,
                        });
                        entry
                    }
                    Err(e) => {
                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                            step_id: step_id.clone(),
                            error: e.detail.clone(),
                        });
                        return Err(e);
                    }
                }
            }

            StepBody::Skill(_) => {
                return Err(AilError {
                    error_type: error_types::PIPELINE_ABORTED,
                    title: "Unsupported step type",
                    detail: format!(
                        "Step '{step_id}' uses a step type not yet implemented in v0.1"
                    ),
                    context: Some(crate::error::ErrorContext::for_step(&session.run_id, &step_id)),
                });
            }
        };

        session.turn_log.append(entry);

        // Evaluate on_result branches.
        if let Some(branches) = &step.on_result {
            let last_entry = session.turn_log.entries().last().expect("just appended");
            if let Some(action) = evaluate_on_result(branches, last_entry) {
                match action {
                    ResultAction::Continue => {}
                    ResultAction::Break => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result break (controlled)"
                        );
                        let outcome = ExecuteOutcome::Break {
                            step_id: step_id.clone(),
                        };
                        let _ = event_tx.send(ExecutorEvent::PipelineCompleted(
                            ExecuteOutcome::Break {
                                step_id: step_id.clone(),
                            },
                        ));
                        return Ok(outcome);
                    }
                    ResultAction::AbortPipeline => {
                        let err = AilError {
                            error_type: error_types::PIPELINE_ABORTED,
                            title: "Pipeline aborted by on_result",
                            detail: format!("Step '{step_id}' on_result fired abort_pipeline"),
                            context: Some(crate::error::ErrorContext::for_step(&session.run_id, &step_id)),
                        };
                        let _ = event_tx.send(ExecutorEvent::PipelineError {
                            error: err.detail.clone(),
                            error_type: err.error_type.to_string(),
                        });
                        return Err(err);
                    }
                    ResultAction::PauseForHuman => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result pause_for_human — waiting for HITL response"
                        );
                        let _ = event_tx.send(ExecutorEvent::HitlGateReached {
                            step_id: step_id.clone(),
                            message: step.message.clone(),
                        });
                        let _response = hitl_rx.recv().unwrap_or_default();
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result HITL gate unblocked — resuming"
                        );
                    }
                    ResultAction::Pipeline {
                        ref path,
                        ref prompt,
                    } => {
                        match execute_sub_pipeline(
                            path,
                            prompt.as_deref(),
                            &step_id,
                            session,
                            runner,
                            1,
                            pipeline_base_dir,
                        ) {
                            Ok(entry) => {
                                session.turn_log.append(entry);
                            }
                            Err(e) => {
                                let _ = event_tx.send(ExecutorEvent::PipelineError {
                                    error: e.detail.clone(),
                                    error_type: e.error_type.to_string(),
                                });
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = event_tx.send(ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed));
    Ok(ExecuteOutcome::Completed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;

    use crate::config::domain::{
        ActionKind, Condition, ContextSource, ResultAction, ResultBranch, ResultMatcher,
        Step, StepBody, StepId,
    };
    use crate::executor::events::{ExecuteOutcome, ExecutionControl, ExecutorEvent};
    use crate::runner::stub::StubRunner;
    use crate::test_helpers::{make_session, prompt_step};

    fn make_control() -> ExecutionControl {
        ExecutionControl {
            kill_requested: Arc::new(AtomicBool::new(false)),
            pause_requested: Arc::new(AtomicBool::new(false)),
            permission_responder: None,
        }
    }

    fn collect_events(rx: mpsc::Receiver<ExecutorEvent>) -> Vec<ExecutorEvent> {
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        events
    }

    // ── Test 1: Empty pipeline → PipelineCompleted event sent ──────────────

    #[test]
    fn empty_pipeline_sends_pipeline_completed_event() {
        let mut session = make_session(vec![]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed)
            )),
            "Expected PipelineCompleted event, got: {events:?}"
        );
    }

    // ── Test 2: kill_requested set before call → pipeline stops early ───────

    #[test]
    fn kill_requested_before_call_skips_all_steps() {
        let mut session = make_session(vec![prompt_step("step1", "do something")]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        control.kill_requested.store(true, Ordering::SeqCst);
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        // No step entries recorded — kill fired before step ran.
        assert_eq!(session.turn_log.entries().len(), 0);
        // PipelineCompleted should still be sent (loop breaks then falls through to it).
        let events = collect_events(rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ExecutorEvent::PipelineCompleted(_))),
            "Expected PipelineCompleted event after kill, got: {events:?}"
        );
    }

    // ── Test 3: Condition::Never → StepSkipped event ────────────────────────

    #[test]
    fn condition_never_sends_step_skipped_event() {
        let step = Step {
            id: StepId("conditional".to_string()),
            body: StepBody::Prompt("should not run".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: Some(Condition::Never),
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 0);

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepSkipped { step_id } if step_id == "conditional"
            )),
            "Expected StepSkipped event, got: {events:?}"
        );
    }

    // ── Test 4: Single prompt step happy path ────────────────────────────────

    #[test]
    fn single_prompt_step_sends_started_and_completed_events() {
        let mut session = make_session(vec![prompt_step("review", "Please review")]);
        let runner = StubRunner::new("looks good");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "review");

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepStarted { step_id, .. } if step_id == "review"
            )),
            "Expected StepStarted event, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepCompleted { step_id, .. } if step_id == "review"
            )),
            "Expected StepCompleted event, got: {events:?}"
        );
    }

    // ── Test 5: Unresolvable template → StepFailed event ───────────────────

    #[test]
    fn unresolvable_template_sends_step_failed_event() {
        let mut session = make_session(vec![prompt_step("bad", "{{ totally.unknown.var }}")]);
        let runner = StubRunner::new("never called");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_err());
        assert_eq!(session.turn_log.entries().len(), 0);

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepFailed { step_id, .. } if step_id == "bad"
            )),
            "Expected StepFailed event, got: {events:?}"
        );
    }

    // ── Test 6: context:shell step → StepStarted + StepCompleted ────────────

    #[test]
    fn context_shell_step_sends_started_and_completed_events() {
        let step = Step {
            id: StepId("shell_ctx".to_string()),
            body: StepBody::Context(ContextSource::Shell("echo hello".to_string())),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "shell_ctx");
        // stdout should contain "hello"
        assert!(session.turn_log.entries()[0]
            .stdout
            .as_deref()
            .unwrap_or("")
            .contains("hello"));

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepStarted { step_id, .. } if step_id == "shell_ctx"
            )),
            "Expected StepStarted event, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepCompleted { step_id, .. } if step_id == "shell_ctx"
            )),
            "Expected StepCompleted event, got: {events:?}"
        );
    }

    // ── Test 7: pause_for_human action step → HitlGateReached then unblocked ─

    #[test]
    fn pause_for_human_sends_hitl_gate_reached_and_completes() {
        let step = Step {
            id: StepId("gate".to_string()),
            body: StepBody::Action(ActionKind::PauseForHuman),
            message: Some("Waiting for approval".to_string()),
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (hitl_tx, hitl_rx) = mpsc::channel::<String>();

        // Send the unblock signal immediately — the main thread will receive it when it blocks.
        hitl_tx.send("approved".to_string()).unwrap();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::HitlGateReached { step_id, .. } if step_id == "gate"
            )),
            "Expected HitlGateReached event, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepCompleted { step_id, .. } if step_id == "gate"
            )),
            "Expected StepCompleted after unblocking, got: {events:?}"
        );
    }

    // ── Test 8: on_result: break branch → PipelineCompleted with Break outcome ─

    #[test]
    fn on_result_break_sends_pipeline_completed_with_break_outcome() {
        let step = Step {
            id: StepId("check".to_string()),
            body: StepBody::Prompt("evaluate".to_string()),
            message: None,
            tools: None,
            on_result: Some(vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Break,
            }]),
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("any response");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Break { step_id } if step_id == "check"));

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::PipelineCompleted(ExecuteOutcome::Break { step_id }) if step_id == "check"
            )),
            "Expected PipelineCompleted(Break) event, got: {events:?}"
        );
    }

    // ── Test 9: Two sequential prompt steps both complete ───────────────────

    #[test]
    fn two_sequential_prompt_steps_both_complete() {
        let mut session = make_session(vec![
            prompt_step("step_a", "First prompt"),
            prompt_step("step_b", "Second prompt"),
        ]);
        let runner = StubRunner::new("ok");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 2);
        assert_eq!(session.turn_log.entries()[0].step_id, "step_a");
        assert_eq!(session.turn_log.entries()[1].step_id, "step_b");

        let events = collect_events(rx);
        let started: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::StepStarted { .. }))
            .collect();
        let completed: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::StepCompleted { .. }))
            .collect();
        assert_eq!(started.len(), 2, "Expected 2 StepStarted events");
        assert_eq!(completed.len(), 2, "Expected 2 StepCompleted events");
    }

    // ── Test 10: pause_requested → pipeline pauses then resumes ─────────────

    #[test]
    fn pause_requested_then_cleared_pipeline_completes() {
        // Use two steps so we have a chance to observe pause behaviour between them.
        let mut session = make_session(vec![
            prompt_step("step_a", "First"),
            prompt_step("step_b", "Second"),
        ]);
        let runner = StubRunner::new("ok");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        // Set pause immediately — the executor will spin-wait between steps.
        let pause_flag = Arc::clone(&control.pause_requested);
        let kill_flag = Arc::clone(&control.kill_requested);

        // Spawn a thread that clears the pause after a short delay.
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            pause_flag.store(false, Ordering::SeqCst);
            // Ensure kill is not set.
            let _ = kill_flag.load(Ordering::SeqCst);
        });

        control.pause_requested.store(true, Ordering::SeqCst);

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        // Both steps should have run after the pause was cleared.
        assert_eq!(session.turn_log.entries().len(), 2);

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed)
            )),
            "Expected PipelineCompleted event after pause+resume"
        );
    }

    // ── Test 11: disabled_steps set → StepSkipped event (not condition check) ─

    #[test]
    fn disabled_step_sends_step_skipped_event() {
        let mut session = make_session(vec![
            prompt_step("enabled", "run me"),
            prompt_step("disabled_step", "skip me"),
        ]);
        let runner = StubRunner::new("ok");
        let control = make_control();
        let mut disabled = HashSet::new();
        disabled.insert("disabled_step".to_string());
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        // Only 1 entry: the enabled step.
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "enabled");

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepSkipped { step_id } if step_id == "disabled_step"
            )),
            "Expected StepSkipped for disabled_step, got: {events:?}"
        );
    }

    // ── Test 12: on_result: abort_pipeline → PipelineError event and Err ─────

    #[test]
    fn on_result_abort_pipeline_sends_pipeline_error_event() {
        let step = Step {
            id: StepId("aborter".to_string()),
            body: StepBody::Prompt("evaluate".to_string()),
            message: None,
            tools: None,
            on_result: Some(vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::AbortPipeline,
            }]),
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("any response");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_err());

        let events = collect_events(rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ExecutorEvent::PipelineError { .. })),
            "Expected PipelineError event, got: {events:?}"
        );
    }

    // ── Test 13: resolved_prompt in StepStarted contains the template value ──

    #[test]
    fn step_started_event_contains_resolved_prompt() {
        let mut session =
            make_session(vec![prompt_step("meta", "Run ID is {{ pipeline.run_id }}")]);
        let run_id = session.run_id.clone();
        let runner = StubRunner::new("ok");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx)
            .unwrap();

        let events = collect_events(rx);
        let started = events
            .iter()
            .find(|e| matches!(e, ExecutorEvent::StepStarted { step_id, .. } if step_id == "meta"));
        assert!(started.is_some(), "Expected StepStarted for 'meta'");
        if let Some(ExecutorEvent::StepStarted {
            resolved_prompt: Some(prompt),
            ..
        }) = started
        {
            assert!(
                prompt.contains(&run_id),
                "Expected run_id in resolved_prompt, got: {prompt}"
            );
        }
    }
}
