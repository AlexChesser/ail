#![allow(clippy::result_large_err)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;

use super::{InvokeOptions, RunResult, Runner, RunnerEvent};
use crate::error::{error_types, AilError};

/// Drives the `claude` CLI in `--output-format stream-json --verbose -p` mode.
///
/// Spike findings (see LEARNINGS.md Phase 8):
/// - `--verbose` is required alongside `--output-format stream-json` when using `-p`.
/// - `CLAUDECODE` must be removed from the child process environment to prevent
///   the nested-session guard from blocking the invocation.
pub struct ClaudeCliRunner {
    pub claude_bin: String,
    /// When true, passes `--dangerously-skip-permissions` to the claude CLI.
    /// Required for headless/automated runs (CI, SWE-bench). See SPEC §8 headless mode.
    pub headless: bool,
}

impl ClaudeCliRunner {
    pub fn new(headless: bool) -> Self {
        ClaudeCliRunner {
            claude_bin: "claude".to_string(),
            headless,
        }
    }

    pub fn with_bin(bin: impl Into<String>, headless: bool) -> Self {
        ClaudeCliRunner {
            claude_bin: bin.into(),
            headless,
        }
    }

    /// Spawn the claude CLI process. Shared by `invoke` and `invoke_streaming`.
    fn spawn_process(
        &self,
        prompt: &str,
        options: &InvokeOptions,
    ) -> Result<std::process::Child, AilError> {
        let mut args: Vec<String> = vec![
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
        ];
        if self.headless {
            args.push("--dangerously-skip-permissions".into());
        }
        if let Some(sid) = &options.resume_session_id {
            args.push("--resume".into());
            args.push(sid.clone());
        }
        if !options.allowed_tools.is_empty() {
            args.push("--allowedTools".into());
            args.push(options.allowed_tools.join(","));
        }
        if !options.denied_tools.is_empty() {
            args.push("--disallowedTools".into());
            args.push(options.denied_tools.join(","));
        }
        if let Some(ref model) = options.model {
            args.push("--model".into());
            args.push(model.clone());
        }
        args.push("-p".into());
        args.push(prompt.to_string());

        let mut cmd = Command::new(&self.claude_bin);
        cmd.args(&args).env_remove("CLAUDECODE");
        if let Some(ref url) = options.base_url {
            cmd.env("ANTHROPIC_BASE_URL", url);
        }
        if let Some(ref token) = options.auth_token {
            cmd.env("ANTHROPIC_AUTH_TOKEN", token);
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "Failed to spawn claude CLI",
                detail: format!("Could not start '{}': {e}", self.claude_bin),
                context: None,
            })
    }
}

impl Default for ClaudeCliRunner {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Runner for ClaudeCliRunner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        let mut child = self.spawn_process(prompt, &options)?;

        let stdout = child.stdout.take().expect("stdout was piped");
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

            let event_type = event["type"].as_str().unwrap_or("");

            match event_type {
                "result" => {
                    let subtype = event["subtype"].as_str().unwrap_or("");
                    if subtype == "error" || event["is_error"].as_bool().unwrap_or(false) {
                        error_detail = Some(
                            event["result"]
                                .as_str()
                                .unwrap_or("unknown error from claude CLI")
                                .to_string(),
                        );
                    } else {
                        result_response = event["result"].as_str().map(|s| s.to_string());
                        result_cost = event["total_cost_usd"].as_f64();
                        result_session_id = event["session_id"].as_str().map(|s| s.to_string());
                    }
                    break;
                }
                "system" | "assistant" | "user" => {
                    // Streaming events — not needed for basic invocation.
                    tracing::debug!(event_type, "stream-json event");
                }
                other => {
                    tracing::warn!(event_type = other, "unexpected stream-json event type");
                }
            }
        }

        let exit_status = child.wait().map_err(|e| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "Failed to wait for claude CLI",
            detail: e.to_string(),
            context: None,
        })?;

        if let Some(detail) = error_detail {
            return Err(AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "claude CLI returned an error result",
                detail,
                context: None,
            });
        }

        if !exit_status.success() {
            return Err(AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "claude CLI exited non-zero",
                detail: format!(
                    "Process exited with {}",
                    exit_status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".to_string())
                ),
                context: None,
            });
        }

        let response = result_response.ok_or_else(|| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "No result event from claude CLI",
            detail: "Stream ended without a 'result' event".to_string(),
            context: None,
        })?;

        Ok(RunResult {
            response,
            cost_usd: result_cost,
            session_id: result_session_id,
        })
    }

    /// Streaming variant — parses `assistant` NDJSON events and emits `RunnerEvent`s.
    /// Sends `StreamDelta` for each text content block, `ToolUse`/`ToolResult` for tool turns,
    /// `CostUpdate` from the final `result` event, then `Completed`.
    fn invoke_streaming(
        &self,
        prompt: &str,
        options: InvokeOptions,
        tx: mpsc::Sender<RunnerEvent>,
    ) -> Result<RunResult, AilError> {
        let mut child = self.spawn_process(prompt, &options)?;

        let stdout = child.stdout.take().expect("stdout was piped");
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

            let event_type = event["type"].as_str().unwrap_or("");

            match event_type {
                "assistant" => {
                    if let Some(content) = event["message"]["content"].as_array() {
                        for item in content {
                            match item["type"].as_str().unwrap_or("") {
                                "text" => {
                                    if let Some(text) = item["text"].as_str() {
                                        if !text.is_empty() {
                                            let _ = tx.send(RunnerEvent::StreamDelta {
                                                text: text.to_string(),
                                            });
                                        }
                                    }
                                }
                                "tool_use" => {
                                    if let Some(name) = item["name"].as_str() {
                                        let _ = tx.send(RunnerEvent::ToolUse {
                                            tool_name: name.to_string(),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    tracing::debug!(event_type, "stream-json assistant event");
                }
                "user" => {
                    // Tool results from the agent feedback loop.
                    if let Some(content) = event["message"]["content"].as_array() {
                        for item in content {
                            if item["type"].as_str() == Some("tool_result") {
                                // tool_use_id could be used to correlate — not needed in MVP.
                                let _ = tx.send(RunnerEvent::ToolResult {
                                    tool_name: String::new(),
                                });
                            }
                        }
                    }
                    tracing::debug!(event_type, "stream-json user event");
                }
                "result" => {
                    let subtype = event["subtype"].as_str().unwrap_or("");
                    if subtype == "error" || event["is_error"].as_bool().unwrap_or(false) {
                        error_detail = Some(
                            event["result"]
                                .as_str()
                                .unwrap_or("unknown error from claude CLI")
                                .to_string(),
                        );
                    } else {
                        result_response = event["result"].as_str().map(|s| s.to_string());
                        result_cost = event["total_cost_usd"].as_f64();
                        result_session_id = event["session_id"].as_str().map(|s| s.to_string());

                        // Emit a cost update so the status bar can show live cost/token data.
                        if let Some(cost) = result_cost {
                            let input_tokens = event["input_tokens"].as_u64().unwrap_or(0);
                            let output_tokens = event["output_tokens"].as_u64().unwrap_or(0);
                            let _ = tx.send(RunnerEvent::CostUpdate {
                                cost_usd: cost,
                                input_tokens,
                                output_tokens,
                            });
                        }
                    }
                    break;
                }
                "system" => {
                    tracing::debug!(event_type, "stream-json system event");
                }
                other => {
                    tracing::warn!(event_type = other, "unexpected stream-json event type");
                }
            }
        }

        let exit_status = child.wait().map_err(|e| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "Failed to wait for claude CLI",
            detail: e.to_string(),
            context: None,
        })?;

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
            let detail = format!(
                "Process exited with {}",
                exit_status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            );
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
        let runner = ClaudeCliRunner::new(false);
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
        let runner = ClaudeCliRunner::new(false);
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
