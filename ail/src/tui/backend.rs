use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;

use ail_core::config::domain::{Pipeline, ProviderConfig};
use ail_core::executor::{self, ExecutionControl, ExecutorEvent};
use ail_core::runner::claude::ClaudeCliRunner;
use ail_core::session::Session;

/// Commands sent from the TUI to the backend thread.
pub enum BackendCommand {
    SubmitPrompt(String),
}

/// Events sent from the backend thread back to the TUI event loop.
#[derive(Debug)]
pub enum BackendEvent {
    Executor(ExecutorEvent),
    /// A fatal error occurred in the backend (e.g. session setup failed).
    Error(String),
}

/// Spawn the background executor thread.
///
/// Returns a command sender (TUI → backend) and an event receiver (backend → TUI).
/// The backend thread owns the `Session` and `Runner` and loops on `BackendCommand`s.
pub fn spawn_backend(
    pipeline: Option<Pipeline>,
    cli_provider: ProviderConfig,
    headless: bool,
) -> (mpsc::Sender<BackendCommand>, mpsc::Receiver<BackendEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<BackendCommand>();
    let (event_tx, event_rx) = mpsc::channel::<BackendEvent>();

    thread::spawn(move || {
        let runner = ClaudeCliRunner::new(headless);
        let resolved_pipeline = pipeline.unwrap_or_else(Pipeline::passthrough);

        for cmd in cmd_rx {
            match cmd {
                BackendCommand::SubmitPrompt(prompt) => {
                    let mut session = Session::new(resolved_pipeline.clone(), prompt.clone());
                    session.cli_provider = cli_provider.clone();

                    let control = ExecutionControl::new();
                    let disabled: HashSet<String> = HashSet::new();

                    // Bridge: executor sends ExecutorEvents; we wrap them into BackendEvents.
                    let (exec_tx, exec_rx) = mpsc::channel::<ExecutorEvent>();
                    let fwd_event_tx = event_tx.clone();
                    let fwd_handle = thread::spawn(move || {
                        for ev in exec_rx {
                            let _ = fwd_event_tx.send(BackendEvent::Executor(ev));
                        }
                    });

                    // Run invocation step via the executor — the passthrough / declared pipeline
                    // already has invocation as step[0] so execute_with_control handles it.
                    match executor::execute_with_control(
                        &mut session,
                        &runner,
                        &control,
                        &disabled,
                        exec_tx,
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
