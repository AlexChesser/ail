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
use tokio::io::AsyncBufRead;

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

/// Apply a `permission_response` control message to the shared allowlist and
/// deliver the decision to any parked responder sender.
///
/// This helper is used by both `spawn_stdin_reader_once` and
/// `spawn_stdin_reader_chat` so the allowlist update rule lives in one place.
///
/// Rules:
/// - On `PermissionResponse::Allow + allow_for_session=true` → insert the
///   parked `display_name` into `allowlist` before delivering the response.
/// - On `Allow + allow_for_session=false` → deliver only; allowlist untouched.
/// - On `Deny(_)` → deliver only; allowlist untouched regardless of the
///   `allow_for_session` flag.
/// - If no sender is parked in `pending`, the response is dropped (no-op).
pub fn apply_permission_response(
    response: PermissionResponse,
    allow_for_session: bool,
    allowlist: &AllowlistArc,
    pending: &PendingPermSlot,
) {
    let mut guard = pending.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((display_name, tx)) = guard.take() {
        if allow_for_session && response == PermissionResponse::Allow {
            if let Ok(mut al) = allowlist.lock() {
                al.insert(display_name);
            }
        }
        let _ = tx.send(response);
    }
}

/// Spawn an async stdin reader for `--once --output-format json` mode.
///
/// Routes:
/// - `HitlResponse`       → `hitl_tx`
/// - `PermissionResponse` → `pending_permission` slot (+ updates allowlist on session-approve)
/// - `Pause`/`Resume`/`Kill` → atomics
///
/// `UserMessage` and `EndSession` are silently ignored in this mode.
///
/// The `reader` parameter is injected so tests can drive the routing logic
/// with an in-memory `Cursor`; callers in production pass
/// `tokio::io::BufReader::new(tokio::io::stdin())`.
pub fn spawn_stdin_reader_once<R>(
    reader: R,
    hitl_tx: mpsc::Sender<String>,
    pending_permission: PendingPermSlot,
    session_allowlist: AllowlistArc,
    pause_requested: Arc<AtomicBool>,
    kill_requested: CancelToken,
) -> tokio::task::JoinHandle<()>
where
    R: AsyncBufRead + Send + Unpin + 'static,
{
    tokio::spawn(async move {
        use ail_core::protocol::{parse_control_message, ControlMessage};
        use tokio::io::AsyncBufReadExt;
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match parse_control_message(&line) {
                Some(ControlMessage::HitlResponse(text)) => {
                    let _ = hitl_tx.send(text);
                }
                Some(ControlMessage::PermissionResponse {
                    response,
                    allow_for_session,
                }) => {
                    apply_permission_response(
                        response,
                        allow_for_session,
                        &session_allowlist,
                        &pending_permission,
                    );
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
/// - `PermissionResponse` → `perm_slot` (same `PendingPermSlot` shape as
///   `--once` mode; `apply_permission_response` updates `session_allowlist`
///   on session-approve)
/// - `Pause`/`Resume`/`Kill` → atomics
///
/// On stdin EOF, sends the None sentinel to `prompt_tx`.
///
/// The `reader` parameter is injected so tests can drive the routing logic
/// with an in-memory `Cursor`; callers in production pass
/// `tokio::io::BufReader::new(tokio::io::stdin())`.
pub fn spawn_stdin_reader_chat<R>(
    reader: R,
    prompt_tx: tokio::sync::mpsc::Sender<Option<String>>,
    hitl_slot: Arc<Mutex<Option<mpsc::SyncSender<String>>>>,
    perm_slot: PendingPermSlot,
    session_allowlist: AllowlistArc,
    pause_requested: Arc<AtomicBool>,
    kill_requested: CancelToken,
) -> tokio::task::JoinHandle<()>
where
    R: AsyncBufRead + Send + Unpin + 'static,
{
    tokio::spawn(async move {
        use ail_core::protocol::{parse_control_message, ControlMessage};
        use tokio::io::AsyncBufReadExt;
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match parse_control_message(&line) {
                Some(ControlMessage::UserMessage(text)) => {
                    let Ok(()) = prompt_tx.send(Some(text)).await else {
                        break;
                    };
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
                Some(ControlMessage::PermissionResponse {
                    response,
                    allow_for_session,
                }) => {
                    apply_permission_response(
                        response,
                        allow_for_session,
                        &session_allowlist,
                        &perm_slot,
                    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use ail_core::runner::PermissionRequest;

    fn req(display: &str) -> PermissionRequest {
        PermissionRequest {
            display_name: display.to_string(),
            display_detail: String::new(),
            tool_input: None,
        }
    }

    #[test]
    fn responder_autoapproves_when_tool_is_in_allowlist() {
        let pending = make_pending_perm();
        let allowlist = make_allowlist();
        allowlist.lock().unwrap().insert("Bash".to_string());

        let responder = make_allowlist_responder(Arc::clone(&pending), Arc::clone(&allowlist));

        let response = responder(req("Bash"));

        assert_eq!(response, PermissionResponse::Allow);
        // The responder must NOT park a sender when the tool is pre-approved.
        assert!(
            pending.lock().unwrap().is_none(),
            "expected pending_permission to stay empty on auto-approve"
        );
    }

    #[test]
    fn apply_permission_response_inserts_on_session_approve() {
        let pending = make_pending_perm();
        let allowlist = make_allowlist();

        // Pre-park a (display_name, SyncSender) as the responder would.
        let (tx, rx) = mpsc::sync_channel(1);
        *pending.lock().unwrap() = Some(("Bash".to_string(), tx));

        apply_permission_response(PermissionResponse::Allow, true, &allowlist, &pending);

        assert!(
            allowlist.lock().unwrap().contains("Bash"),
            "expected 'Bash' to be inserted into the allowlist"
        );
        assert_eq!(rx.try_recv().unwrap(), PermissionResponse::Allow);
        // The slot should be drained after delivery.
        assert!(pending.lock().unwrap().is_none());
    }

    #[test]
    fn apply_permission_response_skips_insert_on_allow_once() {
        let pending = make_pending_perm();
        let allowlist = make_allowlist();

        let (tx, rx) = mpsc::sync_channel(1);
        *pending.lock().unwrap() = Some(("Bash".to_string(), tx));

        apply_permission_response(PermissionResponse::Allow, false, &allowlist, &pending);

        assert!(
            allowlist.lock().unwrap().is_empty(),
            "allow-once must not touch the allowlist"
        );
        assert_eq!(rx.try_recv().unwrap(), PermissionResponse::Allow);
    }

    #[test]
    fn apply_permission_response_ignores_deny_even_when_session_bit_set() {
        let pending = make_pending_perm();
        let allowlist = make_allowlist();

        let (tx, rx) = mpsc::sync_channel(1);
        *pending.lock().unwrap() = Some(("Bash".to_string(), tx));

        // allow_for_session=true but response is Deny — must NOT insert.
        apply_permission_response(
            PermissionResponse::Deny("user rejected".to_string()),
            true,
            &allowlist,
            &pending,
        );

        assert!(
            allowlist.lock().unwrap().is_empty(),
            "deny must never touch the allowlist"
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            PermissionResponse::Deny("user rejected".to_string())
        );
    }

    #[test]
    fn apply_permission_response_is_noop_when_no_sender_parked() {
        let pending = make_pending_perm();
        let allowlist = make_allowlist();

        // No sender parked — this must not panic and must not touch the allowlist.
        apply_permission_response(PermissionResponse::Allow, true, &allowlist, &pending);

        assert!(allowlist.lock().unwrap().is_empty());
        assert!(pending.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn chat_stdin_reader_updates_allowlist_on_session_approve() {
        use std::io::Cursor;
        use tokio::io::BufReader;

        let pending = make_pending_perm();
        let allowlist = make_allowlist();
        let hitl_slot: Arc<Mutex<Option<mpsc::SyncSender<String>>>> = Arc::new(Mutex::new(None));
        let (prompt_tx, _prompt_rx) = tokio::sync::mpsc::channel::<Option<String>>(8);
        let pause = Arc::new(AtomicBool::new(false));
        let kill = CancelToken::new();

        // Pre-park a (display_name, SyncSender) as `make_allowlist_responder`
        // would when a runner requests permission for "Bash".
        let (tx, rx) = mpsc::sync_channel(1);
        *pending.lock().unwrap() = Some(("Bash".to_string(), tx));

        // Feed a single permission_response with allow_for_session=true, then
        // an end_session to make the reader exit cleanly.
        let payload =
            b"{\"type\":\"permission_response\",\"allowed\":true,\"allow_for_session\":true}\n\
                        {\"type\":\"end_session\"}\n"
                .to_vec();

        let handle = spawn_stdin_reader_chat(
            BufReader::new(Cursor::new(payload)),
            prompt_tx,
            hitl_slot,
            Arc::clone(&pending),
            Arc::clone(&allowlist),
            pause,
            kill,
        );
        handle.await.unwrap();

        // The allowlist must now contain "Bash" and the parked sender must
        // have received Allow — proving the chat reader's PermissionResponse
        // arm calls `apply_permission_response` correctly.
        assert!(
            allowlist.lock().unwrap().contains("Bash"),
            "expected chat stdin reader to insert 'Bash' into the allowlist"
        );
        assert_eq!(rx.try_recv().unwrap(), PermissionResponse::Allow);
        assert!(pending.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn once_stdin_reader_updates_allowlist_on_session_approve() {
        use std::io::Cursor;
        use tokio::io::BufReader;

        let pending = make_pending_perm();
        let allowlist = make_allowlist();
        let (hitl_tx, _hitl_rx) = mpsc::channel::<String>();
        let pause = Arc::new(AtomicBool::new(false));
        let kill = CancelToken::new();

        let (tx, rx) = mpsc::sync_channel(1);
        *pending.lock().unwrap() = Some(("Bash".to_string(), tx));

        // EOF terminates the once-mode reader.
        let payload =
            b"{\"type\":\"permission_response\",\"allowed\":true,\"allow_for_session\":true}\n"
                .to_vec();

        let handle = spawn_stdin_reader_once(
            BufReader::new(Cursor::new(payload)),
            hitl_tx,
            Arc::clone(&pending),
            Arc::clone(&allowlist),
            pause,
            kill,
        );
        handle.await.unwrap();

        assert!(allowlist.lock().unwrap().contains("Bash"));
        assert_eq!(rx.try_recv().unwrap(), PermissionResponse::Allow);
    }
}
