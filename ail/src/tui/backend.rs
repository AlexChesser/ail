use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use ail_core::config::domain::{Pipeline, ProviderConfig};
use ail_core::executor::{self, ExecutionControl, ExecutorEvent};
use ail_core::runner::claude::ClaudeInvokeExtensions;
use ail_core::runner::{
    InvokeOptions, PermissionRequest, PermissionResponder, PermissionResponse, Runner, RunnerEvent,
};
use ail_core::session::{Session, TurnEntry};

/// Commands sent from the TUI to the backend thread.
pub enum BackendCommand {
    SubmitPrompt {
        prompt: String,
        disabled_steps: HashSet<String>,
    },
    /// Hot-reload: replace the active pipeline for subsequent runs (i-1).
    SwitchPipeline(Pipeline),
}

/// Events sent from the backend thread back to the TUI event loop.
pub enum BackendEvent {
    Executor(ExecutorEvent),
    /// A fatal error occurred in the backend (e.g. session setup failed).
    Error(String),
    /// Provides the channel to unblock a HITL gate (M10).
    HitlReady(mpsc::Sender<String>),
    /// Provides pause/kill Arcs so the TUI can flip them (M11).
    ControlReady {
        pause: Arc<AtomicBool>,
        kill: Arc<AtomicBool>,
    },
    /// A tool permission request arrived — TUI should show the permission modal (SPEC §13.3).
    PermissionRequest(PermissionRequest),
    /// Provides the sender side of the permission response channel for this run.
    PermReady(mpsc::Sender<PermissionResponse>),
}

/// Spawn the background executor thread.
///
/// Returns a command sender (TUI → backend) and an event receiver (backend → TUI).
pub fn spawn_backend(
    pipeline: Option<Pipeline>,
    cli_provider: ProviderConfig,
    runner: Box<dyn Runner + Send>,
) -> (mpsc::Sender<BackendCommand>, mpsc::Receiver<BackendEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<BackendCommand>();
    let (event_tx, event_rx) = mpsc::channel::<BackendEvent>();

    thread::spawn(move || {
        let mut resolved_pipeline = pipeline.unwrap_or_else(Pipeline::passthrough);

        for cmd in cmd_rx {
            match cmd {
                BackendCommand::SwitchPipeline(new_pipeline) => {
                    resolved_pipeline = new_pipeline;
                }
                BackendCommand::SubmitPrompt {
                    prompt,
                    disabled_steps,
                } => {
                    let mut session = Session::new(resolved_pipeline.clone(), prompt.clone());
                    session.cli_provider = cli_provider.clone();

                    let mut control = ExecutionControl::new();

                    // Send pause/kill Arc clones to the TUI so it can flip them (M11).
                    let _ = event_tx.send(BackendEvent::ControlReady {
                        pause: Arc::clone(&control.pause_requested),
                        kill: Arc::clone(&control.kill_requested),
                    });

                    // Create a hitl channel for this run; send the Sender to the TUI.
                    let (hitl_tx, hitl_rx) = mpsc::channel::<String>();
                    let _ = event_tx.send(BackendEvent::HitlReady(hitl_tx));

                    // Set up permission HITL: send the response channel to the TUI and create
                    // a responder callback. The runner owns the Unix socket lifecycle.
                    let (perm_tx, perm_rx) = mpsc::channel::<PermissionResponse>();
                    let _ = event_tx.send(BackendEvent::PermReady(perm_tx));

                    let perm_event_tx = event_tx.clone();
                    let perm_rx = Arc::new(Mutex::new(perm_rx));
                    let responder: PermissionResponder = Arc::new(move |req: PermissionRequest| {
                        let _ = perm_event_tx.send(BackendEvent::PermissionRequest(req));
                        let rx = perm_rx.lock().unwrap();
                        match rx.recv() {
                            Ok(r) => r,
                            Err(_) => {
                                tracing::error!(
                                    "permission: response channel closed unexpectedly; aborting run"
                                );
                                let _ = perm_event_tx.send(BackendEvent::Error(
                                    "Permission response channel closed unexpectedly. \
                                     The current run has been aborted."
                                        .to_string(),
                                ));
                                PermissionResponse::Deny(
                                    "Permission channel closed; run aborted".to_string(),
                                )
                            }
                        }
                    });

                    control.permission_responder = Some(Arc::clone(&responder));

                    // If the pipeline does not declare an invocation step, run the user's
                    // prompt through the runner before handing off to the executor (SPEC §4.1).
                    let has_invocation_step = resolved_pipeline
                        .steps
                        .first()
                        .map(|s| s.id.as_str() == "invocation")
                        .unwrap_or(false);

                    if !has_invocation_step {
                        let total_steps = resolved_pipeline.steps.len() + 1;
                        let _ = event_tx.send(BackendEvent::Executor(ExecutorEvent::StepStarted {
                            step_id: "invocation".to_string(),
                            step_index: 0,
                            total_steps,
                        }));

                        let invocation_options = InvokeOptions {
                            model: session.cli_provider.model.clone(),
                            extensions: Some(Box::new(ClaudeInvokeExtensions {
                                base_url: session.cli_provider.base_url.clone(),
                                auth_token: session.cli_provider.auth_token.clone(),
                                permission_socket: None,
                            })),
                            permission_responder: Some(Arc::clone(&responder)),
                            cancel_token: Some(Arc::clone(&control.kill_requested)),
                            ..InvokeOptions::default()
                        };

                        let (runner_tx, runner_rx) = mpsc::channel::<RunnerEvent>();
                        let fwd_inv_tx = event_tx.clone();
                        let fwd_inv_handle = thread::spawn(move || {
                            for ev in runner_rx {
                                let _ = fwd_inv_tx
                                    .send(BackendEvent::Executor(ExecutorEvent::RunnerEvent(ev)));
                            }
                        });

                        match runner.invoke_streaming(&prompt, invocation_options, runner_tx) {
                            Ok(result) => {
                                let _ = fwd_inv_handle.join();
                                session.turn_log.append(TurnEntry {
                                    step_id: "invocation".to_string(),
                                    prompt: prompt.clone(),
                                    response: Some(result.response),
                                    timestamp: std::time::SystemTime::now(),
                                    cost_usd: result.cost_usd,
                                    input_tokens: result.input_tokens,
                                    output_tokens: result.output_tokens,
                                    runner_session_id: result.session_id,
                                    stdout: None,
                                    stderr: None,
                                    exit_code: None,
                                });
                                let _ = event_tx.send(BackendEvent::Executor(
                                    ExecutorEvent::StepCompleted {
                                        step_id: "invocation".to_string(),
                                        cost_usd: None,
                                        input_tokens: 0,
                                        output_tokens: 0,
                                    },
                                ));
                            }
                            Err(e) => {
                                let _ = fwd_inv_handle.join();
                                let _ = event_tx.send(BackendEvent::Error(e.detail.clone()));
                                continue;
                            }
                        }
                    }

                    // Bridge: executor sends ExecutorEvents; we wrap them into BackendEvents.
                    let (exec_tx, exec_rx) = mpsc::channel::<ExecutorEvent>();
                    let fwd_event_tx = event_tx.clone();
                    let fwd_handle = thread::spawn(move || {
                        for ev in exec_rx {
                            let _ = fwd_event_tx.send(BackendEvent::Executor(ev));
                        }
                    });

                    match executor::execute_with_control(
                        &mut session,
                        &*runner,
                        &control,
                        &disabled_steps,
                        exec_tx,
                        hitl_rx,
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            let _ = event_tx.send(BackendEvent::Error(e.detail.clone()));
                        }
                    }
                    let _ = fwd_handle.join();
                }
            }
        }
    });

    (cmd_tx, event_rx)
}
