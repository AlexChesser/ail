use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use ail_core::config::domain::{Pipeline, ProviderConfig};
use ail_core::executor::{self, ExecutionControl, ExecutorEvent};
use ail_core::runner::claude::ClaudeCliRunner;
use ail_core::runner::{InvokeOptions, PermissionRequest, PermissionResponse, Runner, RunnerEvent};
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
    headless: bool,
) -> (mpsc::Sender<BackendCommand>, mpsc::Receiver<BackendEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<BackendCommand>();
    let (event_tx, event_rx) = mpsc::channel::<BackendEvent>();

    thread::spawn(move || {
        let runner = ClaudeCliRunner::new(headless);
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

                    // Set up permission HITL (non-headless only): bind a Unix socket,
                    // spawn a listener thread, and send the response channel to the TUI.
                    let perm_socket_path: Option<PathBuf> = if !headless {
                        let path = std::env::temp_dir()
                            .join(format!("ail-perm-{}.sock", uuid::Uuid::new_v4()));
                        Some(path)
                    } else {
                        None
                    };

                    let (perm_tx, perm_rx) = mpsc::channel::<PermissionResponse>();
                    let _ = event_tx.send(BackendEvent::PermReady(perm_tx));

                    // Signal sent by the listener thread once the socket is bound and ready.
                    // We wait for this before spawning Claude CLI to avoid a race where the
                    // MCP bridge tries to connect before the socket exists.
                    let (sock_ready_tx, sock_ready_rx) = mpsc::channel::<()>();

                    let _listener_handle = perm_socket_path.as_ref().map(|path| {
                        let p = path.clone();
                        let etx = event_tx.clone();
                        thread::spawn(move || {
                            let listener = match UnixListener::bind(&p) {
                                Ok(l) => l,
                                Err(e) => {
                                    tracing::error!(error = %e, "permission: failed to bind socket");
                                    return;
                                }
                            };
                            // Signal that the socket is ready before entering the accept loop.
                            let _ = sock_ready_tx.send(());
                            for stream in listener.incoming() {
                                let mut conn = match stream {
                                    Ok(s) => s,
                                    Err(_) => break,
                                };
                                // Read one JSON line — the permission request.
                                let mut reader = BufReader::new(&conn);
                                let mut line = String::new();
                                if reader.read_line(&mut line).is_err() {
                                    continue;
                                }
                                let req_val: serde_json::Value =
                                    match serde_json::from_str(line.trim()) {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };
                                let tool_input = req_val["tool_input"].clone();
                                let perm_req = PermissionRequest {
                                    tool_name: req_val["tool_name"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    tool_input: tool_input.clone(),
                                };
                                let _ = etx.send(BackendEvent::PermissionRequest(perm_req));

                                // Block until the TUI sends a decision.
                                let response = perm_rx
                                    .recv()
                                    .unwrap_or(PermissionResponse::Deny("channel closed".into()));
                                // Claude CLI requires a discriminated union:
                                //   allow → {"behavior":"allow","updatedInput":<original_input>}
                                //   deny  → {"behavior":"deny","message":"<reason>"}
                                let resp_json = match response {
                                    PermissionResponse::Allow => {
                                        serde_json::json!({"behavior": "allow", "updatedInput": tool_input})
                                    }
                                    PermissionResponse::Deny(reason) => {
                                        serde_json::json!({"behavior": "deny", "message": reason})
                                    }
                                };
                                let mut resp_line =
                                    serde_json::to_string(&resp_json).unwrap_or_default();
                                resp_line.push('\n');
                                let _ = conn.write_all(resp_line.as_bytes());
                            }
                        })
                    });

                    // Wait until the socket is bound (or the listener failed and dropped the sender).
                    if perm_socket_path.is_some() {
                        let _ = sock_ready_rx.recv();
                    }

                    control.permission_socket = perm_socket_path.clone();

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
                            base_url: session.cli_provider.base_url.clone(),
                            auth_token: session.cli_provider.auth_token.clone(),
                            permission_socket: perm_socket_path.clone(),
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
                                    runner_session_id: result.session_id,
                                    stdout: None,
                                    stderr: None,
                                    exit_code: None,
                                });
                                let _ = event_tx.send(BackendEvent::Executor(
                                    ExecutorEvent::StepCompleted {
                                        step_id: "invocation".to_string(),
                                        cost_usd: None,
                                    },
                                ));
                            }
                            Err(e) => {
                                let _ = fwd_inv_handle.join();
                                let _ = event_tx.send(BackendEvent::Error(e.detail.clone()));
                                if let Some(path) = perm_socket_path {
                                    let _ = std::fs::remove_file(path);
                                }
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
                        &runner,
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

                    // Clean up the permission socket.
                    if let Some(path) = perm_socket_path {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
    });

    (cmd_tx, event_rx)
}
