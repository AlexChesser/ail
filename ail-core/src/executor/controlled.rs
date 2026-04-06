//! TUI-controlled pipeline execution — `execute_with_control()`.

#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::SystemTime;

use crate::config::domain::{Condition, ContextSource, ResultAction, StepBody};
use crate::error::{error_types, AilError};
use crate::runner::{InvokeOptions, Runner, RunnerEvent};
use crate::session::{Session, TurnEntry};
use crate::template;

use super::events::{ExecuteOutcome, ExecutionControl, ExecutorEvent};
use super::headless::execute_sub_pipeline;
use super::helpers::{
    build_step_runner_box, build_tool_policy, evaluate_on_result, resolve_prompt_file,
    resolve_step_provider, run_shell_command,
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
                    Err(mut e) => {
                        e.context = Some(crate::error::ErrorContext {
                            pipeline_run_id: Some(session.run_id.clone()),
                            step_id: Some(step_id.clone()),
                            source: None,
                        });
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

                // Resolve system_prompt if set
                let resolved_system_prompt = match step
                    .system_prompt
                    .as_deref()
                    .map(|sp| {
                        let content = resolve_prompt_file(sp, &step_id, pipeline_base_dir)?;
                        template::resolve(&content, session).map_err(|mut e| {
                            e.context = Some(crate::error::ErrorContext {
                                pipeline_run_id: Some(session.run_id.clone()),
                                step_id: Some(step_id.clone()),
                                source: None,
                            });
                            e
                        })
                    })
                    .transpose()
                {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                            step_id: step_id.clone(),
                            error: e.detail.clone(),
                        });
                        return Err(e);
                    }
                };

                // Resolve append_system_prompt entries
                let mut resolved_append_system_prompt: Vec<String> = Vec::new();
                if let Some(entries) = &step.append_system_prompt {
                    for entry in entries {
                        let text = match entry {
                            crate::config::domain::SystemPromptEntry::Text(s) => {
                                match template::resolve(s, session) {
                                    Ok(t) => t,
                                    Err(mut e) => {
                                        e.context = Some(crate::error::ErrorContext {
                                            pipeline_run_id: Some(session.run_id.clone()),
                                            step_id: Some(step_id.clone()),
                                            source: None,
                                        });
                                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                                            step_id: step_id.clone(),
                                            error: e.detail.clone(),
                                        });
                                        return Err(e);
                                    }
                                }
                            }
                            crate::config::domain::SystemPromptEntry::File(path) => {
                                let content = match std::fs::read_to_string(path) {
                                    Ok(c) => c,
                                    Err(e) => {
                                        let err = AilError {
                                            error_type: error_types::CONFIG_FILE_NOT_FOUND,
                                            title: "append_system_prompt file not found",
                                            detail: format!(
                                                "Step '{step_id}' append_system_prompt file '{}' could not be read: {e}",
                                                path.display()
                                            ),
                                            context: Some(crate::error::ErrorContext {
                                                pipeline_run_id: Some(session.run_id.clone()),
                                                step_id: Some(step_id.clone()),
                                                source: None,
                                            }),
                                        };
                                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                                            step_id: step_id.clone(),
                                            error: err.detail.clone(),
                                        });
                                        return Err(err);
                                    }
                                };
                                match template::resolve(&content, session) {
                                    Ok(t) => t,
                                    Err(mut e) => {
                                        e.context = Some(crate::error::ErrorContext {
                                            pipeline_run_id: Some(session.run_id.clone()),
                                            step_id: Some(step_id.clone()),
                                            source: None,
                                        });
                                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                                            step_id: step_id.clone(),
                                            error: e.detail.clone(),
                                        });
                                        return Err(e);
                                    }
                                }
                            }
                            crate::config::domain::SystemPromptEntry::Shell(cmd) => {
                                let resolved_cmd = match template::resolve(cmd, session) {
                                    Ok(c) => c,
                                    Err(mut e) => {
                                        e.context = Some(crate::error::ErrorContext {
                                            pipeline_run_id: Some(session.run_id.clone()),
                                            step_id: Some(step_id.clone()),
                                            source: None,
                                        });
                                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                                            step_id: step_id.clone(),
                                            error: e.detail.clone(),
                                        });
                                        return Err(e);
                                    }
                                };
                                match run_shell_command(&session.run_id, &step_id, &resolved_cmd) {
                                    Ok((stdout, _stderr, _exit_code)) => stdout,
                                    Err(e) => {
                                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                                            step_id: step_id.clone(),
                                            error: e.detail.clone(),
                                        });
                                        return Err(e);
                                    }
                                }
                            }
                        };
                        resolved_append_system_prompt.push(text);
                    }
                }

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

                        TurnEntry {
                            step_id: step_id.clone(),
                            prompt: resolved,
                            response: Some(result.response),
                            timestamp: SystemTime::now(),
                            cost_usd: result.cost_usd,
                            input_tokens: result.input_tokens,
                            output_tokens: result.output_tokens,
                            runner_session_id: result.session_id,
                            stdout: None,
                            stderr: None,
                            exit_code: None,
                            thinking: result.thinking,
                            tool_events: result.tool_events,
                        }
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
                TurnEntry {
                    step_id: step_id.clone(),
                    prompt: cmd.clone(),
                    response: None,
                    timestamp: SystemTime::now(),
                    cost_usd: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    runner_session_id: None,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    exit_code: Some(exit_code),
                    thinking: None,
                    tool_events: vec![],
                }
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
                    context: Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id),
                        source: None,
                    }),
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
                            context: Some(crate::error::ErrorContext {
                                pipeline_run_id: Some(session.run_id.clone()),
                                step_id: Some(step_id),
                                source: None,
                            }),
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
