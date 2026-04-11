//! RAII wrapper for the Claude CLI permission HITL bridge.
//!
//! [`ClaudePermissionListener`] owns the full lifecycle of the tool-permission mechanism:
//! binding the local socket, spawning the accept-loop thread, writing the Claude settings
//! file that registers the PreToolUse hooks, and — crucially — tearing everything down on
//! drop via a `__close__` sentinel, thread join, file removal, and socket cleanup.
//!
//! The caller creates a listener, adds `--settings <listener.settings_file()>` to the
//! claude CLI argv, and drops the listener when the invocation completes.

#![allow(clippy::result_large_err)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use interprocess::local_socket::traits::ListenerExt;

use super::super::{PermissionRequest, PermissionResponder, PermissionResponse, RunnerEvent};
use crate::error::AilError;
use crate::fs_util::atomic_write_str;

/// RAII guard for the permission HITL socket and its associated Claude hook settings file.
///
/// Construction blocks until the socket is accepting connections, ensuring the claude CLI
/// can connect to it immediately after spawning.
///
/// On drop, a `__close__` sentinel is sent to the accept-loop thread so it exits cleanly,
/// the thread is joined, the hook settings file is removed, and the socket address is
/// cleaned up.
pub struct ClaudePermissionListener {
    address: String,
    hook_settings_path: PathBuf,
    handle: Option<JoinHandle<()>>,
}

impl ClaudePermissionListener {
    /// Bind a local socket, spawn the accept-loop thread, write the hook settings file,
    /// and block until the socket is ready.
    ///
    /// `responder` is called synchronously in the accept-loop thread for each permission
    /// request from the claude CLI. It blocks until the human decides.
    ///
    /// `event_tx` receives [`RunnerEvent::PermissionRequested`] events so streaming
    /// consumers (TUI) can display the request.
    pub fn start(
        responder: PermissionResponder,
        event_tx: mpsc::Sender<RunnerEvent>,
    ) -> Result<Self, AilError> {
        let address = crate::ipc::generate_address();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();

        let addr = address.clone();
        let handle = thread::spawn(move || {
            let listener = match crate::ipc::bind_local(&addr) {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(error = %e, "permission: failed to bind socket");
                    return;
                }
            };
            let _ = ready_tx.send(());
            for stream in listener.incoming() {
                let mut conn: crate::ipc::IpcStream = match stream {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let mut reader = BufReader::new(&conn);
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let req_val: serde_json::Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                // Shutdown signal sent by Drop after Claude CLI exits.
                // This unblocks listener.incoming() so the thread exits, dropping its clone
                // of event_tx, which allows fwd_handle.join() in execute_with_control to
                // complete and unblock step transitions.
                if req_val.get("__close__").is_some() {
                    break;
                }
                // Translate from Claude's hook JSON to the abstract PermissionRequest.
                let tool_input = req_val["tool_input"].clone();
                let display_name = req_val["tool_name"].as_str().unwrap_or("").to_string();
                let display_detail = format_tool_input_for_display(&tool_input);
                let perm_req = PermissionRequest {
                    display_name,
                    display_detail,
                    tool_input: Some(tool_input.clone()),
                };
                let _ = event_tx.send(RunnerEvent::PermissionRequested(perm_req.clone()));
                let response = responder(perm_req);
                let resp_json = match response {
                    PermissionResponse::Allow => {
                        serde_json::json!({"behavior": "allow", "updatedInput": tool_input})
                    }
                    PermissionResponse::Deny(reason) => {
                        serde_json::json!({"behavior": "deny", "message": reason})
                    }
                };
                let mut resp_line = serde_json::to_string(&resp_json).unwrap_or_default();
                resp_line.push('\n');
                let _ = conn.write_all(resp_line.as_bytes());
            }
            // Socket cleanup happens in Drop, not here, because Drop also needs the address.
        });

        // Block until the socket is accepting connections before returning to the caller.
        let _ = ready_rx.recv();

        let hook_settings_path = write_hook_settings(&address)?;

        Ok(Self {
            address,
            hook_settings_path,
            handle: Some(handle),
        })
    }

    /// The local socket address to pass to hook scripts via `--socket <address>`.
    pub fn socket_address(&self) -> &str {
        &self.address
    }

    /// Path to the Claude settings file that registers the PreToolUse hooks.
    /// Add `--settings <path>` to the claude CLI argv when spawning.
    pub fn settings_file(&self) -> &Path {
        &self.hook_settings_path
    }
}

impl Drop for ClaudePermissionListener {
    fn drop(&mut self) {
        // Send the shutdown sentinel so the accept-loop thread exits cleanly.
        // Without this, the thread's clone of `event_tx` is never dropped, so the
        // forwarding channel in execute_with_control never gets EOF and blocks forever.
        if let Ok(mut s) = crate::ipc::connect_local(&self.address) {
            let _ = s.write_all(b"{\"__close__\":true}\n");
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_file(&self.hook_settings_path);
        crate::ipc::cleanup_address(&self.address);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────────────────────

/// Write a temporary Claude Code settings file registering two PreToolUse hooks.
///
/// - `AskUserQuestion` matcher → `ail ask-user-hook --socket <addr>`
/// - `.*` matcher → `ail check-permission-hook --socket <addr>`
///
/// Both hooks connect to the same local socket. Returns the path to the settings file.
fn write_hook_settings(socket_address: &str) -> Result<PathBuf, AilError> {
    let ail_bin = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("ail"))
        .to_string_lossy()
        .to_string();
    let settings_path =
        std::env::temp_dir().join(format!("ail-settings-{}.json", uuid::Uuid::new_v4()));
    let ask_user_cmd = format!("{ail_bin} ask-user-hook --socket {socket_address}");
    let check_perm_cmd = format!("{ail_bin} check-permission-hook --socket {socket_address}");
    let settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "AskUserQuestion",
                    "hooks": [{ "type": "command", "command": ask_user_cmd }]
                },
                {
                    "matcher": ".*",
                    "hooks": [{ "type": "command", "command": check_perm_cmd }]
                }
            ]
        }
    });
    atomic_write_str(&settings_path, &settings.to_string())?;
    Ok(settings_path)
}

/// Format Claude's `tool_input` JSON into a human-readable detail string for the
/// permission modal. Truncates long values and annotates multi-line strings with line counts.
fn format_tool_input_for_display(tool_input: &serde_json::Value) -> String {
    let mut detail = String::new();
    if let Some(obj) = tool_input.as_object() {
        for (k, v) in obj {
            let val_str = match v {
                serde_json::Value::String(s) => {
                    let lines: Vec<&str> = s.lines().collect();
                    if lines.len() > 1 {
                        format!("{} … ({} lines)", lines[0], lines.len())
                    } else if s.len() > 100 {
                        format!("{}…", &s[..100])
                    } else {
                        s.clone()
                    }
                }
                other => {
                    let s = other.to_string();
                    if s.len() > 100 {
                        format!("{}…", &s[..100])
                    } else {
                        s
                    }
                }
            };
            detail.push_str(&format!("\n    {k}: {val_str}"));
        }
    } else if !tool_input.is_null() {
        detail.push_str(&format!("\n    {tool_input}"));
    }
    detail
}
