use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use ail_core::config::domain::{Pipeline, ProviderConfig};
use ail_core::executor::{self, ExecutionControl, ExecutorEvent};
use ail_core::runner::claude::ClaudeCliRunner;
use ail_core::session::Session;

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

                    let control = ExecutionControl::new();

                    // Send pause/kill Arc clones to the TUI so it can flip them (M11).
                    let _ = event_tx.send(BackendEvent::ControlReady {
                        pause: Arc::clone(&control.pause_requested),
                        kill: Arc::clone(&control.kill_requested),
                    });

                    // Create a hitl channel for this run; send the Sender to the TUI.
                    let (hitl_tx, hitl_rx) = mpsc::channel::<String>();
                    let _ = event_tx.send(BackendEvent::HitlReady(hitl_tx));

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
                }
            }
        }
    });

    (cmd_tx, event_rx)
}
