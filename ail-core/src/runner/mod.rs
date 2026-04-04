//! Runner adapters — the seam between the executor and underlying agent processes.
//!
//! # Architecture
//!
//! The [`Runner`] trait is the single interface the executor sees. Each **agent CLI**
//! (e.g. `claude`, `codex`, `opencode`) gets its own `Runner` implementation that handles
//! its own subprocess lifecycle and stream format:
//!
//! - [`claude::ClaudeCliRunner`] — drives `claude --output-format stream-json --verbose -p`.
//!   Handles Anthropic API, Ollama, Bedrock, and any provider the `claude` CLI supports,
//!   because the CLI normalises upstream differences into one `stream-json` format.
//! - Future runners would live here (e.g. `codex::CodexRunner`, `opencode::OpenCodeRunner`).
//!
//! Provider config (base URL, auth token, model) flows through [`InvokeOptions`] so the
//! executor never names a specific provider. Model-specific output quirks — XML tool calls
//! vs JSON, thinking block structures — are each runner's responsibility.
//!
//! [`stub::StubRunner`] and [`stub::CountingStubRunner`] are deterministic test doubles.

#![allow(clippy::result_large_err)]

pub mod claude;
pub mod factory;
pub mod stub;

use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::error::AilError;

/// A single tool call or tool result event captured during a runner invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEvent {
    /// `"tool_call"` or `"tool_result"`.
    pub event_type: String,
    /// Tool name (e.g. `"Read"`, `"Bash"`). Empty string for tool_result events where
    /// the name is not available in the wire format.
    pub tool_name: String,
    /// Tool call ID from the assistant message (`tool_use.id`) or tool result message
    /// (`tool_result.tool_use_id`).
    pub tool_id: String,
    /// JSON-serialised input (for tool_call) or plain-text/JSON content (for tool_result).
    pub content_json: String,
    /// Monotonically increasing sequence number within this invocation.
    pub seq: i64,
}

/// Result of a single runner invocation.
#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub response: String,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Concatenated thinking/reasoning text from extended thinking blocks, if any.
    /// `None` when no thinking blocks were present in the response.
    pub thinking: Option<String>,
    /// Model name used for this invocation, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Ordered list of tool call and tool result events captured during this invocation.
    pub tool_events: Vec<ToolEvent>,
}

/// A tool permission request emitted by the runner when it requires a human decision before
/// executing a tool.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionRequest {
    /// Human-readable name of the tool being invoked (e.g. `"Bash"`, `"Write"`).
    pub display_name: String,
    /// Human-readable summary of the tool's arguments, pre-formatted by the runner.
    pub display_detail: String,
}

/// The user's decision on a `PermissionRequest`.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionResponse {
    /// Allow the tool to run as-is.
    Allow,
    /// Deny the tool; optional reason shown to the model.
    Deny(String),
}

/// Callback provided to the runner to resolve tool permission requests. The runner owns its
/// transport (MCP, stdio, HTTP, etc.). The callback blocks until the human decides. Runners
/// that do not support tool permissions ignore this field.
pub type PermissionResponder = Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync>;

/// Streaming events emitted by `invoke_streaming()`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerEvent {
    /// A chunk of assistant text arrived.
    StreamDelta { text: String },
    /// A reasoning/thinking block from the model (extended thinking).
    Thinking { text: String },
    /// A tool call was started.
    ToolUse { tool_name: String },
    /// A tool call completed.
    ToolResult { tool_name: String },
    /// Cost / token update.
    CostUpdate {
        cost_usd: f64,
        input_tokens: u64,
        output_tokens: u64,
    },
    /// A tool permission request arrived from the runner.
    PermissionRequested(PermissionRequest),
    /// The invocation completed successfully.
    Completed(RunResult),
    /// The invocation failed.
    Error(String),
}

/// Tool permission policy for a runner invocation (SPEC §5.8).
#[derive(Debug, Clone, Default)]
pub enum ToolPermissionPolicy {
    /// Defer to the runner's default permission behaviour.
    #[default]
    RunnerDefault,
    /// Pre-approve only these tools; all others require a permission decision.
    Allowlist(Vec<String>),
    /// Pre-deny these tools; all others proceed normally.
    Denylist(Vec<String>),
    /// Combine an allowlist and a denylist.
    Mixed {
        allow: Vec<String>,
        deny: Vec<String>,
    },
}

/// Options passed to a runner invocation. Extensible without changing the trait signature.
#[derive(Default)]
pub struct InvokeOptions {
    /// Resumes an existing conversation by session ID. Runners that do not support session
    /// continuity ignore this.
    pub resume_session_id: Option<String>,
    /// Tool permission policy for this invocation (SPEC §5.8).
    pub tool_policy: ToolPermissionPolicy,
    /// Model to use for this invocation (SPEC §15).
    /// Resolved from: pipeline defaults → per-step override → CLI flag (highest priority).
    pub model: Option<String>,
    /// Runner-specific extension data. Callers box a runner-native struct and runners
    /// downcast it. Runners that do not recognise the extension type ignore this field.
    pub extensions: Option<Box<dyn std::any::Any + Send>>,
    /// Callback for bidirectional tool permission prompts. When set, the runner should
    /// intercept permission requests and call this to obtain a decision before proceeding.
    /// Runners that do not support tool permissions ignore this field.
    pub permission_responder: Option<PermissionResponder>,
    /// When set, the runner should abort the in-flight subprocess if this flag becomes `true`.
    /// Callers share the same `Arc<AtomicBool>` used for the kill signal so that CTRL-C and
    /// Ctrl+K both cancel mid-invocation requests. Runners that do not support cancellation
    /// ignore this field.
    pub cancel_token: Option<Arc<AtomicBool>>,
}

pub trait Runner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError>;

    /// Streaming variant — emits `RunnerEvent`s through `tx` as the invocation progresses.
    ///
    /// The default implementation calls `invoke()` and sends a single `Completed` event.
    /// Runners that support real streaming should override this.
    fn invoke_streaming(
        &self,
        prompt: &str,
        options: InvokeOptions,
        tx: std::sync::mpsc::Sender<RunnerEvent>,
    ) -> Result<RunResult, AilError> {
        let result = self.invoke(prompt, options)?;
        let _ = tx.send(RunnerEvent::Completed(result.clone()));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_event_serializes_stream_delta() {
        let event = RunnerEvent::StreamDelta {
            text: "Hello".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "stream_delta");
        assert_eq!(json["text"], "Hello");
    }

    #[test]
    fn runner_event_serializes_cost_update() {
        let event = RunnerEvent::CostUpdate {
            cost_usd: 0.012,
            input_tokens: 100,
            output_tokens: 50,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "cost_update");
        assert_eq!(json["cost_usd"], 0.012);
        assert_eq!(json["input_tokens"], 100);
        assert_eq!(json["output_tokens"], 50);
    }

    #[test]
    fn runner_event_serializes_tool_use() {
        let event = RunnerEvent::ToolUse {
            tool_name: "Bash".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["tool_name"], "Bash");
    }

    #[test]
    fn runner_event_serializes_permission_requested() {
        let event = RunnerEvent::PermissionRequested(PermissionRequest {
            display_name: "Bash".into(),
            display_detail: "rm -rf /tmp/test".into(),
        });
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "permission_requested");
        assert_eq!(json["display_name"], "Bash");
    }

    #[test]
    fn runner_event_serializes_completed() {
        let event = RunnerEvent::Completed(RunResult {
            response: "done".into(),
            cost_usd: Some(0.01),
            session_id: Some("ses_123".into()),
            input_tokens: 10,
            output_tokens: 5,
            thinking: None,
            model: None,
            tool_events: vec![],
        });
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "completed");
        assert_eq!(json["response"], "done");
        assert_eq!(json["cost_usd"], 0.01);
    }

    #[test]
    fn run_result_serializes() {
        let result = RunResult {
            response: "hello".into(),
            cost_usd: None,
            session_id: None,
            input_tokens: 0,
            output_tokens: 0,
            thinking: None,
            model: None,
            tool_events: vec![],
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
        assert_eq!(json["response"], "hello");
        assert!(json["cost_usd"].is_null());
    }
}
