#![allow(clippy::result_large_err)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::traits::ListenerExt; // for .incoming() on LocalSocketListener
use std::process::{Command, Stdio};

use super::{
    InvokeOptions, PermissionRequest, PermissionResponder, RunResult, Runner, RunnerEvent,
    ToolPermissionPolicy,
};
use crate::error::{error_types, AilError};

/// Terminal outcome of parsing a single `stream-json` NDJSON event.
enum StreamParseAction {
    /// Non-terminal event — any `RunnerEvent`s were already sent through `tx`.
    Continue,
    /// The `result` event arrived with a successful response.
    ResultReceived {
        response: Option<String>,
        cost_usd: Option<f64>,
        session_id: Option<String>,
    },
    /// The `result` event arrived indicating an error.
    ResultError(String),
}

/// Parse a single `stream-json` NDJSON event from the claude CLI.
///
/// Sends appropriate `RunnerEvent`s through `tx` (when provided) for streaming display,
/// and returns the terminal `StreamParseAction` to the caller.
///
/// Both `invoke` (pass `None`) and `invoke_streaming` (pass `Some(&tx)`) use this function
/// so the parsing logic stays in one place and is unit-testable without spawning a process.
fn parse_stream_event(
    event: &serde_json::Value,
    tx: Option<&mpsc::Sender<RunnerEvent>>,
) -> StreamParseAction {
    let event_type = event["type"].as_str().unwrap_or("");

    match event_type {
        "assistant" => {
            if let Some(content) = event["message"]["content"].as_array() {
                let block_types: Vec<&str> = content
                    .iter()
                    .map(|item| item["type"].as_str().unwrap_or("unknown"))
                    .collect();
                tracing::debug!(event_type, ?block_types, "stream-json assistant event");
                for item in content {
                    let block_type = item["type"].as_str().unwrap_or("");
                    match block_type {
                        "text" => {
                            if let Some(text) = item["text"].as_str() {
                                if !text.is_empty() {
                                    if let Some(tx) = tx {
                                        let _ = tx.send(RunnerEvent::StreamDelta {
                                            text: text.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        "thinking" => {
                            if let Some(text) = item["thinking"].as_str() {
                                if !text.is_empty() {
                                    if let Some(tx) = tx {
                                        let _ = tx.send(RunnerEvent::Thinking {
                                            text: text.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        "tool_use" => {
                            if let Some(name) = item["name"].as_str() {
                                if let Some(tx) = tx {
                                    let _ = tx.send(RunnerEvent::ToolUse {
                                        tool_name: name.to_string(),
                                    });
                                }
                            }
                        }
                        other => {
                            tracing::debug!(
                                block_type = other,
                                "stream-json: unrecognized assistant content block type"
                            );
                        }
                    }
                }
            } else {
                tracing::debug!(
                    event_type,
                    "stream-json assistant event: message.content is not an array"
                );
            }
            StreamParseAction::Continue
        }
        "user" => {
            if let Some(content) = event["message"]["content"].as_array() {
                for item in content {
                    if item["type"].as_str() == Some("tool_result") {
                        if let Some(tx) = tx {
                            let _ = tx.send(RunnerEvent::ToolResult {
                                tool_name: String::new(),
                            });
                        }
                    }
                }
            }
            tracing::debug!(event_type, "stream-json user event");
            StreamParseAction::Continue
        }
        "result" => {
            let subtype = event["subtype"].as_str().unwrap_or("");
            let is_error = subtype == "error" || event["is_error"].as_bool().unwrap_or(false);
            let result_len = event["result"].as_str().map(|s| s.len());
            let cost = event["total_cost_usd"].as_f64();
            let session_id = event["session_id"].as_str();
            tracing::debug!(
                event_type,
                subtype,
                is_error,
                result_len,
                has_cost = cost.is_some(),
                has_session_id = session_id.is_some(),
                "stream-json result event"
            );
            if is_error {
                StreamParseAction::ResultError(
                    event["result"]
                        .as_str()
                        .unwrap_or("unknown error from claude CLI")
                        .to_string(),
                )
            } else {
                let input_tokens = event["input_tokens"].as_u64().unwrap_or(0);
                let output_tokens = event["output_tokens"].as_u64().unwrap_or(0);
                // Emit a cost update so the status bar can show live cost/token data.
                if let (Some(cost), Some(tx)) = (cost, tx) {
                    let _ = tx.send(RunnerEvent::CostUpdate {
                        cost_usd: cost,
                        input_tokens,
                        output_tokens,
                    });
                }
                StreamParseAction::ResultReceived {
                    response: event["result"].as_str().map(str::to_string),
                    cost_usd: cost,
                    session_id: session_id.map(str::to_string),
                }
            }
        }
        "system" => {
            tracing::debug!(event_type, "stream-json system event");
            StreamParseAction::Continue
        }
        other => {
            tracing::warn!(event_type = other, "unexpected stream-json event type");
            StreamParseAction::Continue
        }
    }
}

/// Runner-specific extensions for `ClaudeCliRunner`, carried in `InvokeOptions::extensions`.
///
/// The executor packs provider config (`base_url`, `auth_token`) here. `ClaudeCliRunner`
/// unpacks them in `spawn_process`. The `permission_socket` field is set by
/// `invoke_streaming` after the Unix socket is created.
///
/// To be cleaned up in task 04 when runner config is fully injected via a dedicated
/// config struct rather than through the generic extensions mechanism.
#[derive(Debug, Clone, Default)]
pub struct ClaudeInvokeExtensions {
    /// Provider base URL — set as `ANTHROPIC_BASE_URL` in the runner subprocess env.
    pub base_url: Option<String>,
    /// Provider auth token — set as `ANTHROPIC_AUTH_TOKEN` in the runner subprocess env.
    pub auth_token: Option<String>,
    /// Address of the local socket created by `invoke_streaming` for permission HITL.
    /// Opaque string — a filesystem path on Unix, a pipe name on Windows.
    /// Set internally by `invoke_streaming`; callers should leave this as `None`.
    pub permission_socket: Option<String>,
}

impl ClaudeInvokeExtensions {
    /// Extract a reference to `ClaudeInvokeExtensions` from `options.extensions`, if present.
    pub fn from_options(opts: &InvokeOptions) -> Option<&Self> {
        opts.extensions
            .as_ref()?
            .downcast_ref::<ClaudeInvokeExtensions>()
    }
}

/// Builder for [`ClaudeCliRunner`]. Encapsulates all Claude-specific construction parameters.
///
/// # Example
/// ```ignore
/// let runner = ClaudeCliRunnerConfig::default().headless(true).build();
/// ```
#[derive(Debug, Clone)]
pub struct ClaudeCliRunnerConfig {
    pub claude_bin: String,
    pub headless: bool,
}

impl Default for ClaudeCliRunnerConfig {
    fn default() -> Self {
        Self {
            claude_bin: "claude".to_string(),
            headless: false,
        }
    }
}

impl ClaudeCliRunnerConfig {
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    pub fn claude_bin(mut self, bin: impl Into<String>) -> Self {
        self.claude_bin = bin.into();
        self
    }

    pub fn build(self) -> ClaudeCliRunner {
        ClaudeCliRunner {
            claude_bin: self.claude_bin,
            headless: self.headless,
        }
    }
}

/// Drives the `claude` CLI in `--output-format stream-json --verbose -p` mode.
///
/// Spike findings (see LEARNINGS.md Phase 8):
/// - `--verbose` is required alongside `--output-format stream-json` when using `-p`.
/// - `CLAUDECODE` must be removed from the child process environment to prevent
///   the nested-session guard from blocking the invocation.
///
/// Construct via [`ClaudeCliRunnerConfig`]:
/// ```ignore
/// let runner = ClaudeCliRunnerConfig::default().headless(true).build();
/// ```
pub struct ClaudeCliRunner {
    pub claude_bin: String,
    /// When true, passes `--dangerously-skip-permissions` to the claude CLI.
    /// Required for headless/automated runs (CI, SWE-bench). See SPEC §8 headless mode.
    pub headless: bool,
}

impl ClaudeCliRunner {
    pub fn from_config(config: ClaudeCliRunnerConfig) -> Self {
        Self {
            claude_bin: config.claude_bin,
            headless: config.headless,
        }
    }

    #[deprecated(note = "Use ClaudeCliRunnerConfig::default().headless(headless).build()")]
    pub fn new(headless: bool) -> Self {
        ClaudeCliRunner {
            claude_bin: "claude".to_string(),
            headless,
        }
    }

    #[deprecated(
        note = "Use ClaudeCliRunnerConfig::default().claude_bin(bin).headless(headless).build()"
    )]
    pub fn with_bin(bin: impl Into<String>, headless: bool) -> Self {
        ClaudeCliRunner {
            claude_bin: bin.into(),
            headless,
        }
    }

    /// Write a temporary MCP config file that points to `ail mcp-bridge` for permission handling.
    ///
    /// Returns the path to the config file; the caller is responsible for deleting it.
    fn write_mcp_config(socket_address: &str) -> Result<PathBuf, AilError> {
        let ail_bin = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("ail"))
            .to_string_lossy()
            .to_string();
        let config_path =
            std::env::temp_dir().join(format!("ail-mcp-config-{}.json", uuid::Uuid::new_v4()));
        let config = serde_json::json!({
            "mcpServers": {
                "ail-permission": {
                    "command": ail_bin,
                    "args": ["mcp-bridge", "--socket", socket_address]
                }
            }
        });
        std::fs::write(&config_path, config.to_string()).map_err(|e| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "Failed to write MCP config",
            detail: format!("Could not write {}: {e}", config_path.display()),
            context: None,
        })?;
        Ok(config_path)
    }

    /// Format Claude's `tool_input` JSON into a human-readable detail string for display
    /// in the permission modal. Truncates long values and shows line counts for multi-line
    /// strings. This is Claude-specific formatting; the abstract `PermissionRequest` carries
    /// the result as `display_detail`.
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
            detail.push_str(&format!("\n    {}", tool_input));
        }
        detail
    }

    /// Bind a local socket, spawn an accept-loop thread, and return the socket address plus a
    /// ready-signal receiver. The thread calls `responder` for each permission request from
    /// Claude CLI and writes the serialised response back over the same connection.
    ///
    /// The caller must wait on the returned `Receiver<()>` before spawning Claude CLI to
    /// avoid a race where the MCP bridge tries to connect before the socket exists.
    ///
    /// Uses `crate::ipc` for cross-platform transport (Unix domain sockets on Unix,
    /// named pipes on Windows).
    fn spawn_permission_listener(
        responder: PermissionResponder,
        event_tx: mpsc::Sender<RunnerEvent>,
    ) -> Result<(String, thread::JoinHandle<()>, mpsc::Receiver<()>), AilError> {
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
                // Retain tool_input for the MCP response; translate to generic display fields.
                let tool_input = req_val["tool_input"].clone();
                let display_name = req_val["tool_name"].as_str().unwrap_or("").to_string();
                let display_detail = Self::format_tool_input_for_display(&tool_input);
                let perm_req = PermissionRequest {
                    display_name,
                    display_detail,
                };
                let _ = event_tx.send(RunnerEvent::PermissionRequested(perm_req.clone()));
                let response = responder(perm_req);
                let resp_json = match response {
                    super::PermissionResponse::Allow => {
                        serde_json::json!({"behavior": "allow", "updatedInput": tool_input})
                    }
                    super::PermissionResponse::Deny(reason) => {
                        serde_json::json!({"behavior": "deny", "message": reason})
                    }
                };
                let mut resp_line = serde_json::to_string(&resp_json).unwrap_or_default();
                resp_line.push('\n');
                let _ = conn.write_all(resp_line.as_bytes());
            }
            crate::ipc::cleanup_address(&addr);
        });
        Ok((address, handle, ready_rx))
    }

    /// Spawn the claude CLI process. Shared by `invoke` and `invoke_streaming`.
    ///
    /// Claude-specific config (`base_url`, `auth_token`, `permission_socket`) is extracted
    /// from `options.extensions` as `ClaudeInvokeExtensions`. `invoke_streaming` sets
    /// `permission_socket` in the extensions before calling this method.
    ///
    /// Returns `(Child, Option<mcp_config_path_to_clean_up>)`.
    fn spawn_process(
        &self,
        prompt: &str,
        options: &InvokeOptions,
    ) -> Result<(std::process::Child, Option<PathBuf>), AilError> {
        let exts = ClaudeInvokeExtensions::from_options(options);
        let base_url = exts.and_then(|e| e.base_url.as_deref());
        let auth_token = exts.and_then(|e| e.auth_token.as_deref());
        let permission_socket: Option<&str> = exts.and_then(|e| e.permission_socket.as_deref());

        let mut args: Vec<String> = vec![
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
        ];
        if self.headless {
            args.push("--dangerously-skip-permissions".into());
        }
        // Only pass --resume for the default Claude API endpoint. Custom providers
        // (Ollama, Bedrock, etc.) have no knowledge of Claude session IDs and will
        // hang waiting to resolve them, causing the pipeline step to time out.
        if let Some(sid) = &options.resume_session_id {
            if base_url.is_none() {
                args.push("--resume".into());
                args.push(sid.clone());
            }
        }
        match &options.tool_policy {
            ToolPermissionPolicy::RunnerDefault => {}
            ToolPermissionPolicy::Allowlist(tools) => {
                args.push("--allowedTools".into());
                args.push(tools.join(","));
            }
            ToolPermissionPolicy::Denylist(tools) => {
                args.push("--disallowedTools".into());
                args.push(tools.join(","));
            }
            ToolPermissionPolicy::Mixed { allow, deny } => {
                args.push("--allowedTools".into());
                args.push(allow.join(","));
                args.push("--disallowedTools".into());
                args.push(deny.join(","));
            }
        }
        if let Some(ref model) = options.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        // Permission HITL: configure MCP bridge when a permission socket is provided and we
        // are not in headless mode. The --permission-prompt-tool mechanism is internal to
        // Claude CLI — the model never sees the bridge tool in its tool list — so this works
        // with both the default Anthropic API and custom providers (Ollama, Bedrock, etc.).
        // The bridge's socket-failure fallback (auto-deny in mcp_bridge.rs) provides a
        // safety net if the connection fails.
        let mcp_config_path = if let Some(socket) = permission_socket {
            if !self.headless {
                let config_path = Self::write_mcp_config(socket)?; // socket is &str
                args.push("--mcp-config".into());
                args.push(config_path.to_string_lossy().to_string());
                args.push("--permission-prompt-tool".into());
                // Claude CLI registers MCP tools as mcp__<server_name>__<tool_name>.
                // Must use the fully qualified name, not the bare tool name.
                args.push("mcp__ail-permission__ail_check_permission".into());
                Some(config_path)
            } else {
                None
            }
        } else {
            None
        };

        args.push("-p".into());
        args.push(prompt.to_string());

        let mut cmd = Command::new(&self.claude_bin);
        cmd.args(&args).env_remove("CLAUDECODE");
        if let Some(url) = base_url {
            cmd.env("ANTHROPIC_BASE_URL", url);
        }
        if let Some(token) = auth_token {
            cmd.env("ANTHROPIC_AUTH_TOKEN", token);
        }
        let child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "Failed to spawn claude CLI",
                detail: format!("Could not start '{}': {e}", self.claude_bin),
                context: None,
            })?;
        Ok((child, mcp_config_path))
    }
}

impl Default for ClaudeCliRunner {
    fn default() -> Self {
        ClaudeCliRunnerConfig::default().build()
    }
}

impl Runner for ClaudeCliRunner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        let (mut child, mcp_config) = self.spawn_process(prompt, &options)?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        // Drain stderr concurrently to prevent pipe-buffer deadlock.
        let stderr_reader = thread::spawn(move || {
            let mut s = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut s);
            s
        });
        let reader = BufReader::new(stdout);

        let mut result_response: Option<String> = None;
        let mut result_cost: Option<f64> = None;
        let mut result_session_id: Option<String> = None;
        let mut error_detail: Option<String> = None;

        for line in reader.lines() {
            let line = line.map_err(|e| AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "Failed to read claude CLI output",
                detail: e.to_string(),
                context: None,
            })?;

            if line.is_empty() {
                continue;
            }

            let event: serde_json::Value = serde_json::from_str(&line).map_err(|e| AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "Malformed JSON from claude CLI",
                detail: format!("Could not parse line: {e}\nLine: {line}"),
                context: None,
            })?;

            tracing::trace!(line = %line, "stream-json raw line");

            match parse_stream_event(&event, None) {
                StreamParseAction::Continue => {}
                StreamParseAction::ResultReceived {
                    response,
                    cost_usd,
                    session_id,
                } => {
                    result_response = response;
                    result_cost = cost_usd;
                    result_session_id = session_id;
                    break;
                }
                StreamParseAction::ResultError(detail) => {
                    error_detail = Some(detail);
                    break;
                }
            }
        }

        let exit_status = child.wait().map_err(|e| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "Failed to wait for claude CLI",
            detail: e.to_string(),
            context: None,
        })?;

        // Wait for the stderr drain thread now that stdout is exhausted.
        let stderr_output = stderr_reader.join().unwrap_or_default();

        if let Some(detail) = error_detail {
            return Err(AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "claude CLI returned an error result",
                detail,
                context: None,
            });
        }

        if !exit_status.success() {
            let code = exit_status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            let detail = if stderr_output.trim().is_empty() {
                format!("Process exited with {code}")
            } else {
                format!("Process exited with {code}: {}", stderr_output.trim())
            };
            tracing::error!(stderr = %stderr_output.trim(), "claude CLI exited non-zero");
            return Err(AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "claude CLI exited non-zero",
                detail,
                context: None,
            });
        }

        let response = result_response.ok_or_else(|| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "No result event from claude CLI",
            detail: "Stream ended without a 'result' event".to_string(),
            context: None,
        })?;

        if let Some(path) = mcp_config {
            let _ = std::fs::remove_file(path);
        }
        Ok(RunResult {
            response,
            cost_usd: result_cost,
            session_id: result_session_id,
        })
    }

    /// Streaming variant — parses `assistant` NDJSON events and emits `RunnerEvent`s.
    /// Sends `StreamDelta` for each text content block, `ToolUse`/`ToolResult` for tool turns,
    /// `CostUpdate` from the final `result` event, then `Completed`.
    ///
    /// If `options.cancel_token` is set, a watchdog thread polls it at 50 ms intervals. When
    /// the flag becomes `true`, the child subprocess is killed and the invocation returns
    /// `RUNNER_CANCELLED`. This is used by CTRL-C and Ctrl+K in the TUI.
    fn invoke_streaming(
        &self,
        prompt: &str,
        mut options: InvokeOptions,
        tx: mpsc::Sender<RunnerEvent>,
    ) -> Result<RunResult, AilError> {
        // If a permission responder is provided and we are not headless, create a Unix socket
        // and set its path in ClaudeInvokeExtensions so spawn_process can configure the
        // MCP bridge. The socket lifecycle is fully encapsulated here.
        if let Some(ref responder) = options.permission_responder {
            if !self.headless {
                let (sock_path, _listener_handle, ready_rx) =
                    Self::spawn_permission_listener(Arc::clone(responder), tx.clone())?;
                let _ = ready_rx.recv();
                // Extract existing extensions (or create defaults) and set the socket.
                let mut exts = options
                    .extensions
                    .as_ref()
                    .and_then(|e| e.downcast_ref::<ClaudeInvokeExtensions>())
                    .cloned()
                    .unwrap_or_default();
                exts.permission_socket = Some(sock_path);
                options.extensions = Some(Box::new(exts));
            }
        }

        let permission_socket_path = ClaudeInvokeExtensions::from_options(&options)
            .and_then(|e| e.permission_socket.clone());

        let (mut child, mcp_config) = self.spawn_process(prompt, &options)?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        // Drain stderr concurrently to prevent pipe-buffer deadlock.
        let stderr_reader = thread::spawn(move || {
            let mut s = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut s);
            s
        });

        // Wrap child in Arc<Mutex> so the watchdog thread can call kill() concurrently.
        let child = Arc::new(Mutex::new(child));

        // Watchdog: polls cancel_token at 50 ms intervals and kills the child if set.
        // The `done` flag signals the watchdog to exit after a normal invocation completes.
        let done = Arc::new(AtomicBool::new(false));
        let watchdog_handle = options.cancel_token.as_ref().map(|token| {
            let token = Arc::clone(token);
            let done_w = Arc::clone(&done);
            let child_w = Arc::clone(&child);
            thread::spawn(move || loop {
                if done_w.load(Ordering::SeqCst) {
                    return;
                }
                if token.load(Ordering::SeqCst) {
                    tracing::info!("cancel_token set — killing runner subprocess");
                    if let Ok(mut c) = child_w.lock() {
                        let _ = c.kill();
                    }
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            })
        });

        let reader = BufReader::new(stdout);

        let mut result_response: Option<String> = None;
        let mut result_cost: Option<f64> = None;
        let mut result_session_id: Option<String> = None;
        let mut error_detail: Option<String> = None;

        // Use match-and-break rather than `?` so the done flag is always set before return.
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    error_detail = Some(e.to_string());
                    break;
                }
            };

            if line.is_empty() {
                continue;
            }

            let event: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    error_detail =
                        Some(format!("Malformed JSON from claude CLI: {e}\nLine: {line}"));
                    break;
                }
            };

            tracing::trace!(line = %line, "stream-json raw line");

            match parse_stream_event(&event, Some(&tx)) {
                StreamParseAction::Continue => {}
                StreamParseAction::ResultReceived {
                    response,
                    cost_usd,
                    session_id,
                } => {
                    result_response = response;
                    result_cost = cost_usd;
                    result_session_id = session_id;
                    break;
                }
                StreamParseAction::ResultError(detail) => {
                    error_detail = Some(detail);
                    break;
                }
            }
        }

        // Signal watchdog to exit and join it (max 50 ms extra latency on normal completion).
        done.store(true, Ordering::SeqCst);
        if let Some(h) = watchdog_handle {
            let _ = h.join();
        }

        // Check for cancellation before interpreting exit status — a killed child exits
        // non-zero, which would otherwise look like an unexpected failure.
        let was_cancelled = options
            .cancel_token
            .as_ref()
            .map(|t| t.load(Ordering::SeqCst))
            .unwrap_or(false);

        let exit_status = child
            .lock()
            .expect("child mutex not poisoned")
            .wait()
            .map_err(|e| AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "Failed to wait for claude CLI",
                detail: e.to_string(),
                context: None,
            })?;

        // Wait for the stderr drain thread now that stdout is exhausted.
        let stderr_output = stderr_reader.join().unwrap_or_default();

        if was_cancelled {
            tracing::info!("runner invocation cancelled by user");
            let _ = tx.send(RunnerEvent::Error("cancelled".to_string()));
            if let Some(path) = mcp_config {
                let _ = std::fs::remove_file(path);
            }
            if let Some(ref addr) = permission_socket_path {
                crate::ipc::cleanup_address(addr);
            }
            return Err(AilError {
                error_type: error_types::RUNNER_CANCELLED,
                title: "Invocation cancelled",
                detail: "Runner subprocess was cancelled by user request".to_string(),
                context: None,
            });
        }

        if let Some(detail) = error_detail {
            let _ = tx.send(RunnerEvent::Error(detail.clone()));
            return Err(AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "claude CLI returned an error result",
                detail,
                context: None,
            });
        }

        if !exit_status.success() {
            let code = exit_status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            let detail = if stderr_output.trim().is_empty() {
                format!("Process exited with {code}")
            } else {
                format!("Process exited with {code}: {}", stderr_output.trim())
            };
            tracing::error!(stderr = %stderr_output.trim(), "claude CLI exited non-zero");
            let _ = tx.send(RunnerEvent::Error(detail.clone()));
            return Err(AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "claude CLI exited non-zero",
                detail,
                context: None,
            });
        }

        let response = result_response.ok_or_else(|| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "No result event from claude CLI",
            detail: "Stream ended without a 'result' event".to_string(),
            context: None,
        })?;

        if let Some(path) = mcp_config {
            let _ = std::fs::remove_file(path);
        }
        if let Some(ref addr) = permission_socket_path {
            crate::ipc::cleanup_address(addr);
        }
        let result = RunResult {
            response,
            cost_usd: result_cost,
            session_id: result_session_id,
        };
        let _ = tx.send(RunnerEvent::Completed(result.clone()));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires: `claude` CLI in PATH, ANTHROPIC_API_KEY set, run outside a
    /// Claude Code session (CLAUDECODE must not be set in the parent env, or
    /// ClaudeCliRunner will unset it for the child).
    ///
    /// Run with: cargo nextest run -- --ignored
    #[test]
    #[ignore]
    fn claude_cli_runner_returns_non_empty_response() {
        let runner = ClaudeCliRunnerConfig::default().build();
        let result = runner
            .invoke(
                "Reply with exactly the word: hello",
                InvokeOptions::default(),
            )
            .unwrap();
        assert!(!result.response.is_empty());
        assert!(result.cost_usd.is_some());
        assert!(result.session_id.is_some());
    }

    #[test]
    #[ignore]
    fn claude_cli_runner_response_contains_expected_text() {
        let runner = ClaudeCliRunnerConfig::default().build();
        let result = runner
            .invoke(
                "Reply with exactly one word: banana",
                InvokeOptions::default(),
            )
            .unwrap();
        assert!(
            result.response.to_lowercase().contains("banana"),
            "Expected 'banana' in response, got: {}",
            result.response
        );
    }
}
