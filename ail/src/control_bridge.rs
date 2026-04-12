//! Shared HITL/permission orchestration helpers.
//!
//! Both `--once --output-format json` and `chat` modes need an allowlist-based
//! `PermissionResponder`, an async stdin reader that routes `ControlMessage`s,
//! and spawn_blocking tasks that drain runner/executor event channels to stdout
//! as NDJSON. This module extracts the common pieces so neither mode duplicates
//! the infrastructure.

use ail_core::executor::ExecutorEvent;
use ail_core::runner::{
    CancelToken, PermissionRequest, PermissionResponder, PermissionResponse, RunnerEvent,
};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

/// Arc-wrapped session allowlist — tool display_names approved for the lifetime of a run.
pub type AllowlistArc = Arc<Mutex<HashSet<String>>>;

/// Pending permission slot used to hand a `SyncSender` from the responder callback to
/// the stdin reader so it can deliver the user's decision.
pub type PendingPermSlot = Arc<Mutex<Option<(String, mpsc::SyncSender<PermissionResponse>)>>>;

/// Build a fresh `AllowlistArc` (empty set).
pub fn make_allowlist() -> AllowlistArc {
    Arc::new(Mutex::new(HashSet::new()))
}

/// Build a fresh `PendingPermSlot` (empty).
pub fn make_pending_perm() -> PendingPermSlot {
    Arc::new(Mutex::new(None))
}

/// Build a `PermissionResponder` that:
/// 1. Auto-approves tools already in `session_allowlist`.
/// 2. Parks a `SyncSender` in `pending_permission` and blocks until the
///    stdin reader delivers a decision (300 s timeout → Deny).
pub fn make_allowlist_responder(
    pending_permission: PendingPermSlot,
    session_allowlist: AllowlistArc,
) -> PermissionResponder {
    Arc::new(move |req: PermissionRequest| {
        if let Ok(guard) = session_allowlist.lock() {
            if guard.contains(&req.display_name) {
                return PermissionResponse::Allow;
            }
        }
        let (tx, rx) = mpsc::sync_channel(1);
        if let Ok(mut guard) = pending_permission.lock() {
            *guard = Some((req.display_name, tx));
        }
        rx.recv_timeout(std::time::Duration::from_secs(300))
            .unwrap_or(PermissionResponse::Deny("timeout".to_string()))
    })
}

/// Emit a single NDJSON line to stdout (locked).
fn emit(value: &serde_json::Value) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = serde_json::to_writer(&mut out, value);
    let _ = writeln!(out);
    let _ = out.flush();
}

/// Spawn a blocking task that drains a `RunnerEvent` channel and emits each event
/// as `{"type":"runner_event","event":<ev>}` NDJSON to stdout.
///
/// Events for tools in `session_allowlist` (auto-approved) are suppressed.
pub fn spawn_runner_event_writer(
    runner_rx: mpsc::Receiver<RunnerEvent>,
    session_allowlist: AllowlistArc,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        for ev in runner_rx {
            if let RunnerEvent::PermissionRequested(ref req) = ev {
                if let Ok(guard) = session_allowlist.lock() {
                    if guard.contains(&req.display_name) {
                        continue;
                    }
                }
            }
            emit(&serde_json::json!({
                "type": "runner_event",
                "event": ev,
            }));
        }
    })
}

/// Spawn a blocking task that drains an `ExecutorEvent` channel and emits each
/// event as NDJSON to stdout.
///
/// When `session_allowlist` is `Some`, `PermissionRequested` events for
/// allowlisted tools are suppressed. Pass `None` when no allowlist is active.
pub fn spawn_executor_event_writer(
    event_rx: mpsc::Receiver<ExecutorEvent>,
    session_allowlist: Option<AllowlistArc>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        for event in event_rx {
            if let Some(ref allowlist) = session_allowlist {
                if let ExecutorEvent::RunnerEvent {
                    event: RunnerEvent::PermissionRequested(ref req),
                } = event
                {
                    if let Ok(guard) = allowlist.lock() {
                        if guard.contains(&req.display_name) {
                            continue;
                        }
                    }
                }
            }
            if let Ok(v) = serde_json::to_value(&event) {
                emit(&v);
            }
        }
    })
}

/// Spawn an async stdin reader for `--once --output-format json` mode.
///
/// Routes:
/// - `HitlResponse`       → `hitl_tx`
/// - `PermissionResponse` → `pending_permission` slot (+ updates allowlist on session-approve)
/// - `Pause`/`Resume`/`Kill` → atomics
///
/// `UserMessage` and `EndSession` are silently ignored in this mode.
pub fn spawn_stdin_reader_once(
    hitl_tx: mpsc::Sender<String>,
    pending_permission: PendingPermSlot,
    session_allowlist: AllowlistArc,
    pause_requested: Arc<AtomicBool>,
    kill_requested: CancelToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use ail_core::protocol::{parse_control_message, ControlMessage};
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match parse_control_message(&line) {
                Some(ControlMessage::HitlResponse(text)) => {
                    let _ = hitl_tx.send(text);
                }
                Some(ControlMessage::PermissionResponse {
                    response,
                    allow_for_session,
                }) => {
                    let mut guard = pending_permission.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some((display_name, tx)) = guard.take() {
                        if allow_for_session && response == PermissionResponse::Allow {
                            if let Ok(mut al) = session_allowlist.lock() {
                                al.insert(display_name);
                            }
                        }
                        let _ = tx.send(response);
                    }
                }
                Some(ControlMessage::Pause) => {
                    pause_requested.store(true, Ordering::SeqCst);
                }
                Some(ControlMessage::Resume) => {
                    pause_requested.store(false, Ordering::SeqCst);
                }
                Some(ControlMessage::Kill) => {
                    kill_requested.cancel();
                }
                _ => {}
            }
        }
    })
}

/// Spawn an async stdin reader for `chat` mode.
///
/// Routes:
/// - `UserMessage`        → `prompt_tx` (Some)
/// - `EndSession`         → `prompt_tx` (None sentinel)
/// - `HitlResponse`       → current-turn sender in `hitl_slot`
/// - `PermissionResponse` → current-turn sender in `perm_slot`
/// - `Pause`/`Resume`/`Kill` → atomics
///
/// On stdin EOF, sends the None sentinel to `prompt_tx`.
pub fn spawn_stdin_reader_chat(
    prompt_tx: tokio::sync::mpsc::Sender<Option<String>>,
    hitl_slot: Arc<Mutex<Option<mpsc::SyncSender<String>>>>,
    perm_slot: Arc<Mutex<Option<mpsc::SyncSender<PermissionResponse>>>>,
    pause_requested: Arc<AtomicBool>,
    kill_requested: CancelToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use ail_core::protocol::{parse_control_message, ControlMessage};
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match parse_control_message(&line) {
                Some(ControlMessage::UserMessage(text)) => {
                    if prompt_tx.send(Some(text)).await.is_err() {
                        break;
                    }
                }
                Some(ControlMessage::EndSession) => {
                    let _ = prompt_tx.send(None).await;
                    break;
                }
                Some(ControlMessage::HitlResponse(text)) => {
                    let guard = hitl_slot.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(ref tx) = *guard {
                        let _ = tx.send(text);
                    }
                }
                Some(ControlMessage::PermissionResponse { response, .. }) => {
                    let guard = perm_slot.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(ref tx) = *guard {
                        let _ = tx.send(response);
                    }
                }
                Some(ControlMessage::Pause) => {
                    pause_requested.store(true, Ordering::SeqCst);
                }
                Some(ControlMessage::Resume) => {
                    pause_requested.store(false, Ordering::SeqCst);
                }
                Some(ControlMessage::Kill) => {
                    kill_requested.cancel();
                }
                None => {}
            }
        }
        // EOF — signal shutdown.
        let _ = prompt_tx.send(None).await;
    })
}
