//! Minimal JSON-RPC 2.0 wire types for the AIL Runner Plugin Protocol.
//!
//! These are purpose-built for the ail ↔ runner-plugin communication channel.
//! We do not use a full JSON-RPC library — this is the minimal subset needed:
//! requests, responses, and notifications over newline-delimited JSON on stdin/stdout.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request sent from ail to the runner plugin (on stdin).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 response from the runner plugin (on stdout).
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Returns true if this response indicates an error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 notification from the runner plugin (on stdout).
///
/// Notifications have no `id` field — they are fire-and-forget.
/// Used for streaming events (deltas, thinking, tool events, etc.).
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// A raw JSON-RPC message read from stdout. Could be a response or notification.
///
/// We distinguish them by the presence of the `id` field:
/// - Has `id` → response to a request we sent
/// - No `id` → notification (streaming event)
#[derive(Debug, Clone, Deserialize)]
pub struct RawJsonRpcMessage {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// Parsed JSON-RPC message — either a response or a notification.
#[derive(Debug, Clone)]
pub enum ParsedMessage {
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

impl RawJsonRpcMessage {
    /// Parse into a typed response or notification based on the presence of `id`.
    pub fn parse(self) -> ParsedMessage {
        if let Some(id) = self.id {
            ParsedMessage::Response(JsonRpcResponse {
                jsonrpc: self.jsonrpc,
                id,
                result: self.result,
                error: self.error,
            })
        } else {
            ParsedMessage::Notification(JsonRpcNotification {
                jsonrpc: self.jsonrpc,
                method: self.method.unwrap_or_default(),
                params: self.params,
            })
        }
    }
}

// ── Protocol method names ────────────────────────────────────────────────────

/// Methods sent from ail to the runner plugin.
pub mod methods {
    /// Handshake: exchange protocol version and discover capabilities.
    pub const INITIALIZE: &str = "initialize";
    /// Send a prompt, receive a response.
    pub const INVOKE: &str = "invoke";
    /// Respond to a tool permission request from the runner.
    pub const PERMISSION_RESPOND: &str = "permission/respond";
    /// Request graceful shutdown.
    pub const SHUTDOWN: &str = "shutdown";
}

/// Notification methods sent from the runner plugin to ail.
pub mod notifications {
    pub const STREAM_DELTA: &str = "stream/delta";
    pub const STREAM_THINKING: &str = "stream/thinking";
    pub const STREAM_TOOL_USE: &str = "stream/tool_use";
    pub const STREAM_TOOL_RESULT: &str = "stream/tool_result";
    pub const STREAM_COST_UPDATE: &str = "stream/cost_update";
    pub const STREAM_PERMISSION_REQUEST: &str = "stream/permission_request";
}

// ── Protocol-level parameter/result types ────────────────────────────────────

/// Parameters for the `initialize` request.
#[derive(Debug, Clone, Serialize)]
pub struct InitializeParams {
    /// The protocol version the host (ail) speaks.
    pub protocol_version: String,
    /// The ail version for informational purposes.
    pub ail_version: String,
}

/// Result of the `initialize` request.
#[derive(Debug, Clone, Deserialize)]
pub struct InitializeResult {
    /// Runner's declared name (e.g. "codex").
    pub name: String,
    /// Runner extension version.
    pub version: String,
    /// Protocol version the runner speaks.
    pub protocol_version: String,
    /// Declared capabilities.
    pub capabilities: PluginCapabilities,
}

/// Capabilities declared by the runner plugin during `initialize`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PluginCapabilities {
    /// Runner emits streaming notifications during invoke.
    #[serde(default)]
    pub streaming: bool,
    /// Runner can resume sessions by ID.
    #[serde(default)]
    pub session_resume: bool,
    /// Runner emits tool_use / tool_result events.
    #[serde(default)]
    pub tool_events: bool,
    /// Runner sends permission_request notifications and awaits permission/respond.
    #[serde(default)]
    pub permission_requests: bool,
}

/// Parameters for the `invoke` request.
#[derive(Debug, Clone, Serialize)]
pub struct InvokeParams {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<serde_json::Value>,
}

/// Result of the `invoke` request — maps directly to RunResult fields.
#[derive(Debug, Clone, Deserialize)]
pub struct InvokeResult {
    pub response: String,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_events: Vec<super::super::ToolEvent>,
}

/// Parameters for the `permission/respond` request.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionRespondParams {
    pub allow: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Notification parameter types ─────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct StreamDeltaParams {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamThinkingParams {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamToolUseParams {
    pub tool_name: String,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamToolResultParams {
    pub tool_name: String,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamCostUpdateParams {
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamPermissionRequestParams {
    pub display_name: String,
    pub display_detail: String,
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_jsonrpc_version() {
        let req = JsonRpcRequest::new(1, "initialize", None);
        let json: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "initialize");
        assert!(json.get("params").is_none());
    }

    #[test]
    fn request_serializes_with_params() {
        let params = serde_json::json!({"protocol_version": "1"});
        let req = JsonRpcRequest::new(1, "initialize", Some(params.clone()));
        let json: serde_json::Value = serde_json::to_value(&req).unwrap();
        assert_eq!(json["params"], params);
    }

    #[test]
    fn response_deserializes_success() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"name":"codex"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
        assert!(!resp.is_error());
    }

    #[test]
    fn response_deserializes_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_error());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn raw_message_parses_as_response_when_id_present() {
        let json = r#"{"jsonrpc":"2.0","id":5,"result":{}}"#;
        let raw: RawJsonRpcMessage = serde_json::from_str(json).unwrap();
        match raw.parse() {
            ParsedMessage::Response(resp) => assert_eq!(resp.id, 5),
            ParsedMessage::Notification(_) => panic!("expected response"),
        }
    }

    #[test]
    fn raw_message_parses_as_notification_when_no_id() {
        let json = r#"{"jsonrpc":"2.0","method":"stream/delta","params":{"text":"hello"}}"#;
        let raw: RawJsonRpcMessage = serde_json::from_str(json).unwrap();
        match raw.parse() {
            ParsedMessage::Notification(n) => {
                assert_eq!(n.method, "stream/delta");
            }
            ParsedMessage::Response(_) => panic!("expected notification"),
        }
    }

    #[test]
    fn initialize_result_deserializes() {
        let json = r#"{
            "name": "codex",
            "version": "0.1.0",
            "protocol_version": "1",
            "capabilities": {
                "streaming": true,
                "session_resume": false,
                "tool_events": false,
                "permission_requests": false
            }
        }"#;
        let result: InitializeResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.name, "codex");
        assert!(result.capabilities.streaming);
        assert!(!result.capabilities.session_resume);
    }

    #[test]
    fn invoke_params_serializes() {
        let params = InvokeParams {
            prompt: "hello".into(),
            session_id: Some("ses-1".into()),
            model: None,
            system_prompt: None,
            tool_policy: None,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["prompt"], "hello");
        assert_eq!(json["session_id"], "ses-1");
        assert!(json.get("model").is_none());
    }
}
