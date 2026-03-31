#![allow(clippy::result_large_err)]

pub mod claude;
pub mod stub;

use crate::error::AilError;

/// Result of a single runner invocation.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub response: String,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
}

/// A tool permission request intercepted from Claude CLI via the MCP permission bridge (SPEC §13.3).
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// The name of the tool Claude wants to use (e.g. `"Bash"`, `"Write"`).
    pub tool_name: String,
    /// The input arguments Claude would pass to the tool.
    pub tool_input: serde_json::Value,
}

/// The user's decision on a `PermissionRequest`.
#[derive(Debug, Clone)]
pub enum PermissionResponse {
    /// Allow the tool to run as-is.
    Allow,
    /// Deny the tool; optional reason shown to the model.
    Deny(String),
}

/// Streaming events emitted by `invoke_streaming()`.
#[derive(Debug, Clone)]
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
    /// A tool permission request arrived via the MCP bridge (SPEC §13.3).
    PermissionRequested(PermissionRequest),
    /// The invocation completed successfully.
    Completed(RunResult),
    /// The invocation failed.
    Error(String),
}

/// Options passed to a runner invocation. Extensible without changing the trait signature.
#[derive(Default)]
pub struct InvokeOptions {
    /// Resumes an existing conversation by session ID (passed as `--resume <id>`).
    pub resume_session_id: Option<String>,
    /// Tools pre-approved for this step — passed as `--allowedTools` (SPEC §5.8).
    pub allowed_tools: Vec<String>,
    /// Tools pre-denied for this step — passed as `--disallowedTools` (SPEC §5.8).
    pub denied_tools: Vec<String>,
    /// Model to use for this invocation — passed as `--model` to the runner (SPEC §15).
    /// Resolved from: pipeline defaults → per-step override → CLI flag (highest priority).
    pub model: Option<String>,
    /// Provider base URL — set as `ANTHROPIC_BASE_URL` in the runner subprocess env (SPEC §15).
    pub base_url: Option<String>,
    /// Provider auth token — set as `ANTHROPIC_AUTH_TOKEN` in the runner subprocess env (SPEC §15).
    pub auth_token: Option<String>,
    /// Unix socket path for bidirectional permission prompts via the MCP bridge (SPEC §13.3).
    /// When set (non-headless), the runner writes an MCP config pointing to `ail mcp-bridge`
    /// and passes `--permission-prompt-tool ail_check_permission` to Claude CLI.
    pub permission_socket: Option<std::path::PathBuf>,
}

pub trait Runner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError>;

    /// Whether this runner requires a Unix permission socket for HITL permission prompts.
    ///
    /// Returns `false` by default. `ClaudeCliRunner` overrides this based on `headless`.
    fn needs_permission_socket(&self) -> bool {
        false
    }

    /// Streaming variant — emits `RunnerEvent`s through `tx` as the invocation progresses.
    ///
    /// The default implementation calls `invoke()` and sends a single `Completed` event.
    /// Runners that support real streaming (e.g. `ClaudeCliRunner`) should override this.
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
