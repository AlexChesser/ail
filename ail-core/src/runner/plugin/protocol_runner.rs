//! ProtocolRunner — generic Runner implementation that speaks the AIL Runner Plugin Protocol.
//!
//! Spawns a plugin executable as a subprocess, communicates via JSON-RPC 2.0 over
//! stdin/stdout, and translates the wire format to/from ail-core's `RunResult`/`RunnerEvent`.

#![allow(clippy::result_large_err)]

use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use tracing::{debug, warn};

use super::jsonrpc::{
    self, InitializeParams, InitializeResult, InvokeParams, InvokeResult, InvokeSamplingParams,
    JsonRpcRequest, ParsedMessage, PermissionRespondParams, PluginCapabilities, RawJsonRpcMessage,
    StreamCostUpdateParams, StreamDeltaParams, StreamPermissionRequestParams, StreamThinkingParams,
    StreamToolResultParams, StreamToolUseParams,
};
use super::manifest::PluginManifest;
use super::subprocess_spec_from_manifest;
use crate::error::AilError;
use crate::runner::subprocess::SubprocessSession;
use crate::runner::{
    InvokeOptions, PermissionRequest, PermissionResponse, RunResult, Runner, RunnerEvent,
    ToolPermissionPolicy,
};

/// A runner that communicates with any AIL Runner Protocol-compliant executable.
pub struct ProtocolRunner {
    manifest: PluginManifest,
}

impl ProtocolRunner {
    pub fn new(manifest: PluginManifest) -> Self {
        Self { manifest }
    }
}

impl Runner for ProtocolRunner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        let (tx, _rx) = std::sync::mpsc::channel();
        self.invoke_streaming(prompt, options, tx)
    }

    fn invoke_streaming(
        &self,
        prompt: &str,
        options: InvokeOptions,
        tx: Sender<RunnerEvent>,
    ) -> Result<RunResult, AilError> {
        let spec = subprocess_spec_from_manifest(&self.manifest);
        let cancel_token = options.cancel_token.clone().unwrap_or_default();

        let mut subprocess = SubprocessSession::spawn(spec, Some(cancel_token.clone()))?;

        // Take stdin — we need to write to the child
        let mut stdin = subprocess.take_stdin().ok_or_else(|| {
            AilError::plugin_protocol_error("failed to open stdin to plugin process")
        })?;

        // Take stdout for reading responses
        let stdout = subprocess.take_stdout().ok_or_else(|| {
            AilError::plugin_protocol_error("failed to open stdout from plugin process")
        })?;
        let reader = BufReader::new(stdout);

        let id_counter = AtomicU64::new(1);

        // 1. Initialize handshake
        let init_params = InitializeParams {
            protocol_version: "1".to_string(),
            ail_version: crate::version().to_string(),
        };
        let init_id = id_counter.fetch_add(1, Ordering::SeqCst);
        let init_req = JsonRpcRequest::new(
            init_id,
            jsonrpc::methods::INITIALIZE,
            Some(serde_json::to_value(&init_params).map_err(|e| {
                AilError::plugin_protocol_error(format!("failed to serialize init params: {e}"))
            })?),
        );

        send_request(&mut stdin, &init_req)?;
        let (capabilities, mut lines) = read_initialize_response(reader, init_id)?;

        debug!(
            runner = %self.manifest.name,
            streaming = capabilities.streaming,
            session_resume = capabilities.session_resume,
            "plugin initialized"
        );

        // 2. Send invoke request
        let invoke_id = id_counter.fetch_add(1, Ordering::SeqCst);
        let tool_policy_json = serialize_tool_policy(&options.tool_policy);
        let invoke_params = InvokeParams {
            prompt: prompt.to_string(),
            session_id: options.resume_session_id.clone(),
            model: options.model.clone(),
            system_prompt: options.system_prompt.clone(),
            tool_policy: tool_policy_json,
            sampling: options.sampling.as_ref().map(|s| InvokeSamplingParams {
                temperature: s.temperature,
                top_p: s.top_p,
                top_k: s.top_k,
                max_tokens: s.max_tokens,
                stop_sequences: s.stop_sequences.clone(),
                thinking: s.thinking,
            }),
        };
        let invoke_req = JsonRpcRequest::new(
            invoke_id,
            jsonrpc::methods::INVOKE,
            Some(serde_json::to_value(&invoke_params).map_err(|e| {
                AilError::plugin_protocol_error(format!("failed to serialize invoke params: {e}"))
            })?),
        );

        send_request(&mut stdin, &invoke_req)?;

        // 3. Read streaming notifications and the final response
        let result = read_invoke_response(
            &mut lines,
            invoke_id,
            &tx,
            &options,
            &mut stdin,
            &id_counter,
        )?;

        // 4. Send shutdown (best-effort)
        let shutdown_id = id_counter.fetch_add(1, Ordering::SeqCst);
        let shutdown_req = JsonRpcRequest::new(shutdown_id, jsonrpc::methods::SHUTDOWN, None);
        let _ = send_request(&mut stdin, &shutdown_req);
        drop(stdin);

        // 5. Reap the subprocess
        let outcome = subprocess.finish()?;
        if outcome.was_cancelled {
            return Err(AilError::runner_cancelled(
                "plugin invocation was cancelled",
            ));
        }

        let _ = tx.send(RunnerEvent::Completed(result.clone()));
        Ok(result)
    }
}

// ── Wire helpers ─────────────────────────────────────────────────────────────

fn send_request(stdin: &mut impl Write, req: &JsonRpcRequest) -> Result<(), AilError> {
    let line = serde_json::to_string(req).map_err(|e| {
        AilError::plugin_protocol_error(format!("failed to serialize request: {e}"))
    })?;
    writeln!(stdin, "{}", line).map_err(|e| {
        AilError::plugin_protocol_error(format!("failed to write to plugin stdin: {e}"))
    })?;
    stdin.flush().map_err(|e| {
        AilError::plugin_protocol_error(format!("failed to flush plugin stdin: {e}"))
    })?;
    Ok(())
}

/// Read lines from stdout until we get the initialize response.
fn read_initialize_response<R: BufRead>(
    reader: R,
    expected_id: u64,
) -> Result<(PluginCapabilities, std::io::Lines<R>), AilError> {
    let mut lines = reader.lines();

    loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(e)) => {
                return Err(AilError::plugin_protocol_error(format!(
                    "failed to read from plugin stdout: {e}"
                )));
            }
            None => {
                return Err(AilError::plugin_protocol_error(
                    "plugin closed stdout before sending initialize response",
                ));
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let raw: RawJsonRpcMessage = serde_json::from_str(&line).map_err(|e| {
            AilError::plugin_protocol_error(format!(
                "invalid JSON-RPC from plugin: {e} (line: {line})"
            ))
        })?;

        match raw.parse() {
            ParsedMessage::Response(resp) => {
                if resp.id != expected_id {
                    warn!(
                        expected = expected_id,
                        got = resp.id,
                        "unexpected response id during initialize"
                    );
                    continue;
                }
                if let Some(err) = resp.error {
                    return Err(AilError::plugin_protocol_error(format!(
                        "plugin initialize failed: {} (code {})",
                        err.message, err.code
                    )));
                }
                let result_value = resp.result.ok_or_else(|| {
                    AilError::plugin_protocol_error("plugin initialize response has no result")
                })?;
                let init_result: InitializeResult =
                    serde_json::from_value(result_value).map_err(|e| {
                        AilError::plugin_protocol_error(format!("invalid initialize result: {e}"))
                    })?;
                return Ok((init_result.capabilities, lines));
            }
            ParsedMessage::Notification(n) => {
                debug!(method = %n.method, "ignoring notification during initialize");
                continue;
            }
        }
    }
}

/// Read streaming notifications and the final invoke response.
fn read_invoke_response<R: BufRead>(
    lines: &mut std::io::Lines<R>,
    expected_id: u64,
    tx: &Sender<RunnerEvent>,
    options: &InvokeOptions,
    stdin: &mut impl Write,
    id_counter: &AtomicU64,
) -> Result<RunResult, AilError> {
    loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(e)) => {
                return Err(AilError::plugin_protocol_error(format!(
                    "failed to read from plugin stdout: {e}"
                )));
            }
            None => {
                return Err(AilError::plugin_protocol_error(
                    "plugin closed stdout before sending invoke response",
                ));
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let raw: RawJsonRpcMessage = serde_json::from_str(&line).map_err(|e| {
            AilError::plugin_protocol_error(format!(
                "invalid JSON-RPC from plugin: {e} (line: {})",
                truncate(&line, 200)
            ))
        })?;

        match raw.parse() {
            ParsedMessage::Response(resp) => {
                if resp.id != expected_id {
                    debug!(
                        expected = expected_id,
                        got = resp.id,
                        "unexpected response id"
                    );
                    continue;
                }
                if let Some(err) = resp.error {
                    return Err(AilError::runner_failed(format!(
                        "plugin invoke failed: {} (code {})",
                        err.message, err.code
                    )));
                }
                let result_value = resp.result.ok_or_else(|| {
                    AilError::plugin_protocol_error("plugin invoke response has no result")
                })?;
                let invoke_result: InvokeResult =
                    serde_json::from_value(result_value).map_err(|e| {
                        AilError::plugin_protocol_error(format!("invalid invoke result: {e}"))
                    })?;
                return Ok(RunResult {
                    response: invoke_result.response,
                    cost_usd: invoke_result.cost_usd,
                    session_id: invoke_result.session_id,
                    input_tokens: invoke_result.input_tokens,
                    output_tokens: invoke_result.output_tokens,
                    thinking: invoke_result.thinking,
                    model: invoke_result.model,
                    tool_events: invoke_result.tool_events,
                });
            }
            ParsedMessage::Notification(n) => {
                handle_notification(&n.method, n.params, tx, options, stdin, id_counter);
            }
        }
    }
}

/// Dispatch a streaming notification to the appropriate RunnerEvent.
fn handle_notification(
    method: &str,
    params: Option<serde_json::Value>,
    tx: &Sender<RunnerEvent>,
    options: &InvokeOptions,
    stdin: &mut impl Write,
    id_counter: &AtomicU64,
) {
    let params = params.unwrap_or(serde_json::Value::Null);

    match method {
        jsonrpc::notifications::STREAM_DELTA => {
            if let Ok(p) = serde_json::from_value::<StreamDeltaParams>(params) {
                let _ = tx.send(RunnerEvent::StreamDelta { text: p.text });
            }
        }
        jsonrpc::notifications::STREAM_THINKING => {
            if let Ok(p) = serde_json::from_value::<StreamThinkingParams>(params) {
                let _ = tx.send(RunnerEvent::Thinking { text: p.text });
            }
        }
        jsonrpc::notifications::STREAM_TOOL_USE => {
            if let Ok(p) = serde_json::from_value::<StreamToolUseParams>(params) {
                let _ = tx.send(RunnerEvent::ToolUse {
                    tool_name: p.tool_name,
                    tool_use_id: p.tool_use_id,
                    input: p.input,
                });
            }
        }
        jsonrpc::notifications::STREAM_TOOL_RESULT => {
            if let Ok(p) = serde_json::from_value::<StreamToolResultParams>(params) {
                let _ = tx.send(RunnerEvent::ToolResult {
                    tool_name: p.tool_name,
                    tool_use_id: p.tool_use_id,
                    content: p.content,
                    is_error: p.is_error,
                });
            }
        }
        jsonrpc::notifications::STREAM_COST_UPDATE => {
            if let Ok(p) = serde_json::from_value::<StreamCostUpdateParams>(params) {
                let _ = tx.send(RunnerEvent::CostUpdate {
                    cost_usd: p.cost_usd,
                    input_tokens: p.input_tokens,
                    output_tokens: p.output_tokens,
                });
            }
        }
        jsonrpc::notifications::STREAM_PERMISSION_REQUEST => {
            if let Ok(p) = serde_json::from_value::<StreamPermissionRequestParams>(params) {
                handle_permission_request(p, options, stdin, id_counter, tx);
            }
        }
        other => {
            debug!(
                method = other,
                "unknown notification from plugin — ignoring"
            );
        }
    }
}

/// Handle a permission request from the plugin.
fn handle_permission_request(
    params: StreamPermissionRequestParams,
    options: &InvokeOptions,
    stdin: &mut impl Write,
    id_counter: &AtomicU64,
    tx: &Sender<RunnerEvent>,
) {
    let request = PermissionRequest {
        display_name: params.display_name,
        display_detail: params.display_detail,
        tool_input: params.tool_input,
    };

    // If we have a permission responder, use it
    let response = if let Some(ref responder) = options.permission_responder {
        responder(request)
    } else {
        // No responder — emit event and default deny
        let _ = tx.send(RunnerEvent::PermissionRequested(request));
        PermissionResponse::Deny("no permission handler configured".to_string())
    };

    // Send the permission response back to the plugin
    let (allow, reason) = match response {
        PermissionResponse::Allow => (true, None),
        PermissionResponse::Deny(r) => (false, Some(r)),
    };

    let resp_id = id_counter.fetch_add(1, Ordering::SeqCst);
    let resp_params = PermissionRespondParams { allow, reason };
    let req = JsonRpcRequest::new(
        resp_id,
        jsonrpc::methods::PERMISSION_RESPOND,
        serde_json::to_value(&resp_params).ok(),
    );
    let _ = send_request(stdin, &req);
}

fn serialize_tool_policy(policy: &ToolPermissionPolicy) -> Option<serde_json::Value> {
    match policy {
        ToolPermissionPolicy::RunnerDefault => None,
        ToolPermissionPolicy::NoTools => Some(serde_json::json!({"type": "no_tools"})),
        ToolPermissionPolicy::Allowlist(tools) => {
            Some(serde_json::json!({"type": "allowlist", "tools": tools}))
        }
        ToolPermissionPolicy::Denylist(tools) => {
            Some(serde_json::json!({"type": "denylist", "tools": tools}))
        }
        ToolPermissionPolicy::Mixed { allow, deny } => {
            Some(serde_json::json!({"type": "mixed", "allow": allow, "deny": deny}))
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
