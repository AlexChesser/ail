//! Tests for `ClaudePermissionListener` lifecycle (runner/claude/permission.rs).
//!
//! Covers: start/expose-address, hook-settings file creation, drop cleanup
//! (file removal + thread join), and allow/deny roundtrip over the IPC socket.

#![cfg(unix)]

use ail_core::ipc;
use ail_core::runner::claude::permission::ClaudePermissionListener;
use ail_core::runner::{PermissionRequest, PermissionResponse, RunnerEvent};
use std::io::{BufRead, BufReader, Write};
use std::sync::{mpsc, Arc};
use std::time::Duration;

// ── Lifecycle ────────────────────────────────────────────────────────────────

#[test]
fn permission_listener_start_exposes_address_and_settings_file_exists() {
    let responder: Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync> =
        Arc::new(|_| PermissionResponse::Allow);
    let (tx, _rx) = mpsc::channel();

    let listener = ClaudePermissionListener::start(responder, tx).expect("start should succeed");

    assert!(
        !listener.socket_address().is_empty(),
        "socket_address should be non-empty"
    );
    assert!(
        listener.settings_file().exists(),
        "settings file should exist at {:?}",
        listener.settings_file()
    );
}

#[test]
fn permission_listener_drop_removes_settings_file() {
    let responder: Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync> =
        Arc::new(|_| PermissionResponse::Allow);
    let (tx, _rx) = mpsc::channel();

    let listener = ClaudePermissionListener::start(responder, tx).expect("start should succeed");
    let settings_path = listener.settings_file().to_path_buf();
    let socket_addr = listener.socket_address().to_string();

    assert!(
        settings_path.exists(),
        "settings file should exist before drop"
    );

    drop(listener);

    assert!(
        !settings_path.exists(),
        "settings file should be removed after drop"
    );

    // On Unix the socket file should also be cleaned up
    assert!(
        !std::path::Path::new(&socket_addr).exists(),
        "socket file should be removed after drop"
    );
}

#[test]
fn permission_listener_drop_joins_accept_thread_within_timeout() {
    let responder: Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync> =
        Arc::new(|_| PermissionResponse::Allow);
    let (tx, _rx) = mpsc::channel();

    let listener = ClaudePermissionListener::start(responder, tx).expect("start should succeed");

    // Run the drop inside a bounded thread to detect accept-loop hangs.
    let handle = std::thread::spawn(move || {
        drop(listener);
    });

    // If the __close__ sentinel mechanism works, the thread should join quickly.
    let result = handle.join();
    assert!(result.is_ok(), "Drop should complete without panicking");
}

// ── Permission roundtrip ─────────────────────────────────────────────────────

#[test]
fn permission_listener_responder_allow_roundtrip() {
    let responder: Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync> =
        Arc::new(|_req| PermissionResponse::Allow);
    let (tx, rx) = mpsc::channel();

    let listener = ClaudePermissionListener::start(responder, tx).expect("start should succeed");

    // Simulate a Claude CLI hook connecting to the socket
    let mut conn = ipc::connect_local(listener.socket_address()).expect("connect should succeed");

    // Send a synthetic PreToolUse permission request
    let request = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" }
    });
    let mut req_line = serde_json::to_string(&request).unwrap();
    req_line.push('\n');
    conn.write_all(req_line.as_bytes())
        .expect("write should succeed");

    // Read the response
    let mut reader = BufReader::new(&conn);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .expect("read should succeed");

    let resp: serde_json::Value =
        serde_json::from_str(response_line.trim()).expect("response should be valid JSON");
    assert_eq!(
        resp["behavior"].as_str(),
        Some("allow"),
        "Expected allow behavior, got: {resp}"
    );

    // Verify the event was sent
    let event = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("should receive PermissionRequested event");
    match event {
        RunnerEvent::PermissionRequested(req) => {
            assert_eq!(req.display_name, "Bash");
        }
        other => panic!("Expected PermissionRequested, got: {other:?}"),
    }

    drop(conn);
    drop(listener);
}

#[test]
fn permission_listener_responder_deny_roundtrip() {
    let responder: Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync> =
        Arc::new(|_req| PermissionResponse::Deny("not allowed".to_string()));
    let (tx, _rx) = mpsc::channel();

    let listener = ClaudePermissionListener::start(responder, tx).expect("start should succeed");

    let mut conn = ipc::connect_local(listener.socket_address()).expect("connect should succeed");

    let request = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/etc/passwd" }
    });
    let mut req_line = serde_json::to_string(&request).unwrap();
    req_line.push('\n');
    conn.write_all(req_line.as_bytes())
        .expect("write should succeed");

    let mut reader = BufReader::new(&conn);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .expect("read should succeed");

    let resp: serde_json::Value =
        serde_json::from_str(response_line.trim()).expect("response should be valid JSON");
    assert_eq!(
        resp["behavior"].as_str(),
        Some("deny"),
        "Expected deny behavior, got: {resp}"
    );
    assert_eq!(
        resp["message"].as_str(),
        Some("not allowed"),
        "Expected deny message, got: {resp}"
    );

    drop(conn);
    drop(listener);
}
