#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::SystemTime;

use serde::Serialize;

use crate::config::domain::{
    ContextSource, ExitCodeMatch, ResultAction, ResultMatcher, StepBody, MAX_SUB_PIPELINE_DEPTH,
};
use crate::error::{error_types, AilError};
use crate::runner::claude::ClaudeInvokeExtensions;
use crate::runner::{
    InvokeOptions, PermissionResponder, Runner, RunnerEvent, ToolPermissionPolicy,
};
use crate::session::{Session, TurnEntry};
use crate::template;

/// Returned by `execute()` to distinguish successful completion variants.
#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ExecuteOutcome {
    /// All steps ran to completion.
    Completed,
    /// A `break` action fired; remaining steps were skipped. This is not an error.
    Break { step_id: String },
    /// An error occurred during execution.
    Error(String),
}

/// Signals the executor can receive from the TUI while a pipeline is running.
pub struct ExecutionControl {
    /// Set to `true` to request a pause between steps. The executor spin-waits until cleared.
    pub pause_requested: Arc<AtomicBool>,
    /// Set to `true` to request that the executor stop immediately after the current step.
    pub kill_requested: Arc<AtomicBool>,
    /// Callback for tool permission HITL via the MCP bridge (SPEC §13.3).
    /// Propagated into `InvokeOptions::permission_responder` for each runner invocation.
    pub permission_responder: Option<PermissionResponder>,
}

impl ExecutionControl {
    pub fn new() -> Self {
        ExecutionControl {
            pause_requested: Arc::new(AtomicBool::new(false)),
            kill_requested: Arc::new(AtomicBool::new(false)),
            permission_responder: None,
        }
    }
}

impl Default for ExecutionControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Events emitted by `execute_with_control()` to the TUI.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutorEvent {
    StepStarted {
        step_id: String,
        step_index: usize,
        total_steps: usize,
        /// The resolved prompt that will be sent to the runner.
        /// `None` for non-prompt steps (context:shell, action, sub-pipeline).
        resolved_prompt: Option<String>,
    },
    StepCompleted {
        step_id: String,
        cost_usd: Option<f64>,
        input_tokens: u64,
        output_tokens: u64,
        /// The runner's response text.
        /// `None` for non-prompt steps (context:shell, action, sub-pipeline).
        response: Option<String>,
        /// Model used for this step, if available.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    StepSkipped {
        step_id: String,
    },
    StepFailed {
        step_id: String,
        error: String,
    },
    /// A `pause_for_human` step was reached — executor is blocked until `hitl_rx` receives a value.
    HitlGateReached {
        step_id: String,
        /// Optional operator-facing message from the step's `message:` YAML field.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// A streaming event from the runner, nested under `event` so the inner `type` field is
    /// preserved in the NDJSON output. Using a named field avoids the internally-tagged
    /// newtype-of-tagged-enum serialization conflict that would overwrite the inner `type`.
    RunnerEvent {
        event: RunnerEvent,
    },
    /// Pipeline completed. The `outcome` field (`"completed"` or `"break"`) comes from
    /// `ExecuteOutcome`'s own `#[serde(tag = "outcome")]`, merged into this object by serde.
    PipelineCompleted(ExecuteOutcome),
    /// Pipeline aborted with an error.
    PipelineError {
        error: String,
        error_type: String,
    },
}

/// Load and run a sub-pipeline, returning a `TurnEntry` for the calling step.
///
/// The `path_template` may contain `{{ variable }}` syntax (SPEC §11); it is resolved
/// against `session` before the file is loaded. The sub-pipeline runs in isolation:
/// a fresh `Session` is created with the parent's `last_response` as its invocation prompt.
/// The child's final step response becomes the returned entry's `response` field.
///
/// `depth` guards against infinite recursion; exceeding `MAX_SUB_PIPELINE_DEPTH` aborts.
fn execute_sub_pipeline(
    path_template: &str,
    step_id: &str,
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
) -> Result<TurnEntry, AilError> {
    if depth >= MAX_SUB_PIPELINE_DEPTH {
        return Err(AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Sub-pipeline depth limit exceeded",
            detail: format!(
                "Step '{step_id}' would exceed the maximum sub-pipeline nesting depth of {MAX_SUB_PIPELINE_DEPTH}"
            ),
            context: Some(crate::error::ErrorContext {
                pipeline_run_id: Some(session.run_id.clone()),
                step_id: Some(step_id.to_string()),
                source: None,
            }),
        });
    }

    // Resolve template variables in the path (SPEC §11).
    let resolved_path = template::resolve(path_template, session).map_err(|mut e| {
        e.context = Some(crate::error::ErrorContext {
            pipeline_run_id: Some(session.run_id.clone()),
            step_id: Some(step_id.to_string()),
            source: None,
        });
        e
    })?;

    let path = std::path::Path::new(&resolved_path);
    let sub_pipeline = crate::config::load(path).map_err(|mut e| {
        e.context = Some(crate::error::ErrorContext {
            pipeline_run_id: Some(session.run_id.clone()),
            step_id: Some(step_id.to_string()),
            source: None,
        });
        e
    })?;

    // The sub-pipeline's invocation prompt is the parent's most recent response.
    let invocation_prompt = session
        .turn_log
        .last_response()
        .unwrap_or(&session.invocation_prompt)
        .to_string();

    let mut child_session = crate::session::Session::new(sub_pipeline, invocation_prompt);
    child_session.cli_provider = session.cli_provider.clone();

    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        sub_pipeline = %resolved_path,
        depth,
        "executing sub-pipeline"
    );

    execute_inner(&mut child_session, runner, depth + 1)?;

    let response = child_session
        .turn_log
        .last_response()
        .unwrap_or("")
        .to_string();

    Ok(TurnEntry {
        step_id: step_id.to_string(),
        prompt: resolved_path,
        response: Some(response),
        timestamp: SystemTime::now(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        runner_session_id: child_session
            .turn_log
            .last_runner_session_id()
            .map(str::to_string),
        stdout: None,
        stderr: None,
        exit_code: None,
        thinking: None,
        tool_events: vec![],
    })
}

/// Inner recursive executor used by both `execute()` and sub-pipeline calls.
/// `depth` tracks nesting level to enforce `MAX_SUB_PIPELINE_DEPTH`.
fn execute_inner(
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
) -> Result<ExecuteOutcome, AilError> {
    if session.pipeline.steps.is_empty() {
        tracing::info!(run_id = %session.run_id, "empty pipeline — no steps to execute");
        return Ok(ExecuteOutcome::Completed);
    }

    // Clone to avoid borrow conflict when calling execute_sub_pipeline (&mut session)
    // while iterating step bodies.
    let steps: Vec<_> = session.pipeline.steps.clone();

    for step in &steps {
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

                let resume_id = session
                    .turn_log
                    .last_runner_session_id()
                    .map(|s| s.to_string());

                session.turn_log.record_step_started(&step_id, &resolved);

                let resolved_provider = session
                    .pipeline
                    .defaults
                    .clone()
                    .merge(crate::config::domain::ProviderConfig {
                        model: step.model.clone(),
                        base_url: None,
                        auth_token: None,
                        input_cost_per_1k: None,
                        output_cost_per_1k: None,
                    })
                    .merge(session.cli_provider.clone());

                let tool_policy = build_tool_policy(step.tools.as_ref());
                let options = InvokeOptions {
                    resume_session_id: resume_id,
                    tool_policy,
                    model: resolved_provider.model,
                    extensions: Some(Box::new(ClaudeInvokeExtensions {
                        base_url: resolved_provider.base_url,
                        auth_token: resolved_provider.auth_token,
                        permission_socket: None,
                    })),
                    permission_responder: None,
                    cancel_token: None,
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
                tracing::info!(run_id = %session.run_id, step_id = %step_id, "pause_for_human");
                // v0.1: pause_for_human is a no-op in headless/--once mode.
                continue;
            }

            StepBody::SubPipeline(path_template) => {
                session
                    .turn_log
                    .record_step_started(&step_id, path_template);
                execute_sub_pipeline(path_template, &step_id, session, runner, depth)?
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
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result pause_for_human (no-op in uncontrolled execution)"
                        );
                    }
                    ResultAction::Pipeline(ref path_template) => {
                        let entry =
                            execute_sub_pipeline(path_template, &step_id, session, runner, depth)?;
                        session.turn_log.append(entry);
                    }
                }
            }
        }
    }

    Ok(ExecuteOutcome::Completed)
}

/// Execute all steps in `session.pipeline` in order.
///
/// SPEC §4.2 core invariant: once execution begins, all steps run in order.
/// Early exit only via explicit declared outcomes — never silent failures.
pub fn execute(session: &mut Session, runner: &dyn Runner) -> Result<ExecuteOutcome, AilError> {
    execute_inner(session, runner, 0)
}

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

        let entry = match &step.body {
            StepBody::Prompt(template_text) => {
                let template_text = match resolve_prompt_file(template_text, &step_id) {
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

                let resume_id = session
                    .turn_log
                    .last_runner_session_id()
                    .map(|s| s.to_string());

                session.turn_log.record_step_started(&step_id, &resolved);

                let resolved_provider = session
                    .pipeline
                    .defaults
                    .clone()
                    .merge(crate::config::domain::ProviderConfig {
                        model: step.model.clone(),
                        base_url: None,
                        auth_token: None,
                        input_cost_per_1k: None,
                        output_cost_per_1k: None,
                    })
                    .merge(session.cli_provider.clone());

                let tool_policy = build_tool_policy(step.tools.as_ref());
                let options = InvokeOptions {
                    resume_session_id: resume_id,
                    tool_policy,
                    model: resolved_provider.model,
                    extensions: Some(Box::new(ClaudeInvokeExtensions {
                        base_url: resolved_provider.base_url,
                        auth_token: resolved_provider.auth_token,
                        permission_socket: None,
                    })),
                    permission_responder: control.permission_responder.clone(),
                    cancel_token: Some(Arc::clone(&control.kill_requested)),
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

                let invoke_result = runner.invoke_streaming(&resolved, options, runner_tx);
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
                        let detail = e.detail.clone();
                        let _ = event_tx.send(ExecutorEvent::StepFailed {
                            step_id: step_id.clone(),
                            error: detail,
                        });
                        return Err(e);
                    }
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

            StepBody::SubPipeline(path_template) => {
                session
                    .turn_log
                    .record_step_started(&step_id, path_template);
                match execute_sub_pipeline(path_template, &step_id, session, runner, 0) {
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
                    ResultAction::Pipeline(ref path_template) => {
                        match execute_sub_pipeline(path_template, &step_id, session, runner, 0) {
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
            return Some(branch.action.clone());
        }
    }
    None
}

/// Build a `ToolPermissionPolicy` from an optional `ToolPolicy` domain value.
fn build_tool_policy(tools: Option<&crate::config::domain::ToolPolicy>) -> ToolPermissionPolicy {
    match tools {
        Some(t) if !t.allow.is_empty() && !t.deny.is_empty() => ToolPermissionPolicy::Mixed {
            allow: t.allow.clone(),
            deny: t.deny.clone(),
        },
        Some(t) if !t.allow.is_empty() => ToolPermissionPolicy::Allowlist(t.allow.clone()),
        Some(t) if !t.deny.is_empty() => ToolPermissionPolicy::Denylist(t.deny.clone()),
        _ => ToolPermissionPolicy::RunnerDefault,
    }
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
            defaults: Default::default(),
            timeout_seconds: None,
        };
        Session::new(pipeline, "invocation prompt".to_string())
    }

    fn prompt_step(id: &str, text: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt(text.to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
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
        ])
        .with_pipeline("subpipeline");
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

    #[test]
    fn executor_event_serializes_step_started() {
        let event = ExecutorEvent::StepStarted {
            step_id: "review".into(),
            step_index: 0,
            total_steps: 3,
            resolved_prompt: Some("Please review the code".into()),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "step_started");
        assert_eq!(json["step_id"], "review");
        assert_eq!(json["step_index"], 0);
        assert_eq!(json["total_steps"], 3);
    }

    #[test]
    fn executor_event_serializes_step_completed() {
        let event = ExecutorEvent::StepCompleted {
            step_id: "review".into(),
            cost_usd: Some(0.003),
            input_tokens: 100,
            output_tokens: 50,
            response: Some("Looks good!".into()),
            model: None,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "step_completed");
        assert_eq!(json["cost_usd"], 0.003);
        assert_eq!(json["input_tokens"], 100);
        assert_eq!(json["output_tokens"], 50);
    }

    #[test]
    fn executor_event_serializes_pipeline_completed() {
        let event = ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed);
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "pipeline_completed");
        assert_eq!(json["outcome"], "completed");
    }

    #[test]
    fn executor_event_serializes_runner_event_with_nested_event_field() {
        let event = ExecutorEvent::RunnerEvent {
            event: RunnerEvent::StreamDelta {
                text: "hello".into(),
            },
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "runner_event");
        assert_eq!(json["event"]["type"], "stream_delta");
        assert_eq!(json["event"]["text"], "hello");
    }

    #[test]
    fn executor_event_serializes_pipeline_error_with_fields() {
        let event = ExecutorEvent::PipelineError {
            error: "something went wrong".into(),
            error_type: "PIPELINE_ABORTED".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "pipeline_error");
        assert_eq!(json["error"], "something went wrong");
        assert_eq!(json["error_type"], "PIPELINE_ABORTED");
    }

    #[test]
    fn execute_outcome_serializes_break() {
        let outcome = ExecuteOutcome::Break {
            step_id: "s1".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&outcome).unwrap()).unwrap();
        assert_eq!(json["outcome"], "break");
        assert_eq!(json["step_id"], "s1");
    }
}
