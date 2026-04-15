//! `ClaudeCliRunner` — drives `claude --output-format stream-json --verbose -p`.
//!
//! Handles Anthropic API, Ollama, Bedrock, and any provider the `claude` CLI supports,
//! because the CLI normalises upstream differences into one `stream-json` format.
//!
//! The runner is composed of three focused helpers:
//! - [`decoder::ClaudeNdjsonDecoder`] — stateful NDJSON stream decoder, no process coupling.
//! - [`permission::ClaudePermissionListener`] — RAII guard for the tool-permission socket.
//! - [`crate::runner::subprocess::SubprocessSession`] — generic subprocess lifecycle.

#![allow(clippy::result_large_err)]

pub mod decoder;
pub mod permission;
mod wire_dto;

use std::io::BufRead;
use std::sync::mpsc;

use decoder::ClaudeNdjsonDecoder;
use permission::ClaudePermissionListener;

use super::subprocess::{SubprocessSession, SubprocessSpec};
use super::{InvokeOptions, RunResult, Runner, RunnerEvent, ToolPermissionPolicy};
use crate::error::AilError;

// ── Extension types ───────────────────────────────────────────────────────────────────────────

/// Runner-specific extensions for `ClaudeCliRunner`, carried in `InvokeOptions::extensions`.
///
/// The executor packs provider config (`base_url`, `auth_token`) here via
/// [`Runner::build_extensions`]. `ClaudeCliRunner` unpacks them in `build_subprocess_spec`.
///
/// To be cleaned up in task 04 when runner config is fully injected via a dedicated
/// config struct rather than through the generic extensions mechanism.
#[derive(Debug, Clone, Default)]
pub struct ClaudeInvokeExtensions {
    /// Provider base URL — set as `ANTHROPIC_BASE_URL` in the runner subprocess env.
    pub base_url: Option<String>,
    /// Provider auth token — set as `ANTHROPIC_AUTH_TOKEN` in the runner subprocess env.
    pub auth_token: Option<String>,
}

impl ClaudeInvokeExtensions {
    /// Extract a reference to `ClaudeInvokeExtensions` from `options.extensions`, if present.
    pub fn from_options(opts: &InvokeOptions) -> Option<&Self> {
        opts.extensions
            .as_ref()?
            .downcast_ref::<ClaudeInvokeExtensions>()
    }
}

// ── Config and runner struct ──────────────────────────────────────────────────────────────────

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

    // ── Private helpers ───────────────────────────────────────────────────────────────────────

    /// Translate `InvokeOptions` (and an optional permission settings file) into a
    /// [`SubprocessSpec`] ready to pass to [`SubprocessSession::spawn`].
    ///
    /// Provider config (`base_url`, `auth_token`) is extracted from
    /// `options.extensions` as [`ClaudeInvokeExtensions`].
    fn build_subprocess_spec(
        &self,
        prompt: &str,
        options: &InvokeOptions,
        hook_settings_file: Option<&std::path::Path>,
    ) -> SubprocessSpec {
        fn quantize_effort(thinking: Option<f64>) -> Option<&'static str> {
            // SPEC §30.4.2 — quartile quantization of `thinking: [0.0, 1.0]`
            // to Claude CLI `--effort <low|medium|high|max>`. `0.0` omits the
            // flag (let the CLI default apply).
            let t = thinking?;
            if t <= 0.0 {
                None
            } else if t <= 0.25 {
                Some("low")
            } else if t <= 0.50 {
                Some("medium")
            } else if t <= 0.75 {
                Some("high")
            } else {
                Some("max")
            }
        }
        let exts = ClaudeInvokeExtensions::from_options(options);
        let base_url = exts.and_then(|e| e.base_url.as_deref());
        let auth_token = exts.and_then(|e| e.auth_token.as_deref());

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
            if let Some(url) = base_url {
                tracing::warn!(
                    base_url = %url,
                    session_id = %sid,
                    "resume suppressed: --resume is only supported on the default Claude API \
                     endpoint; non-default base_url detected. The step will run as a fresh \
                     session. Set resume: false on this step to silence this warning."
                );
            } else {
                args.push("--resume".into());
                args.push(sid.clone());
            }
        }

        match &options.tool_policy {
            ToolPermissionPolicy::RunnerDefault => {}
            ToolPermissionPolicy::NoTools => {
                args.push("--tools".into());
                args.push("".into());
            }
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
        if let Some(ref sp) = options.system_prompt {
            args.push("--system-prompt".into());
            args.push(sp.clone());
        }
        for entry in &options.append_system_prompt {
            args.push("--append-system-prompt".into());
            args.push(entry.clone());
        }

        // ── Sampling (SPEC §30.4.2) ──────────────────────────────────────────
        //
        // Claude CLI exposes only `--effort` (low|medium|high|max) for sampling
        // control. Every other sampling field is warned-and-ignored — SPEC
        // §30.4.1 requires warn-not-error so pipelines remain portable.
        if let Some(s) = options.sampling.as_ref() {
            if let Some(level) = quantize_effort(s.thinking) {
                args.push("--effort".into());
                args.push(level.to_string());
            }
            if s.temperature.is_some() {
                tracing::warn!(
                    "ClaudeCliRunner: sampling.temperature is not supported by the \
                     claude CLI; ignoring"
                );
            }
            if s.top_p.is_some() {
                tracing::warn!(
                    "ClaudeCliRunner: sampling.top_p is not supported by the claude \
                     CLI; ignoring"
                );
            }
            if s.top_k.is_some() {
                tracing::warn!(
                    "ClaudeCliRunner: sampling.top_k is not supported by the claude \
                     CLI; ignoring"
                );
            }
            if s.max_tokens.is_some() {
                tracing::warn!(
                    "ClaudeCliRunner: sampling.max_tokens is not supported by the \
                     claude CLI; ignoring (this is distinct from --max-budget-usd, \
                     which is a dollar cap, not a token cap)"
                );
            }
            if s.stop_sequences.is_some() {
                tracing::warn!(
                    "ClaudeCliRunner: sampling.stop_sequences is not supported by \
                     the claude CLI; ignoring"
                );
            }
        }

        // Permission HITL: register PreToolUse hooks when a settings file is provided.
        // The file is written by ClaudePermissionListener::start() and contains the socket
        // address. The hooks intercept AskUserQuestion and all other tool calls before they
        // execute — the model never sees the hooks.
        if let Some(path) = hook_settings_file {
            if !self.headless {
                args.push("--settings".into());
                args.push(path.to_string_lossy().to_string());
            }
        }

        args.push("-p".into());
        args.push(prompt.to_string());

        let mut env_set = Vec::new();
        if let Some(url) = base_url {
            env_set.push(("ANTHROPIC_BASE_URL".into(), url.to_string()));
        }
        if let Some(token) = auth_token {
            env_set.push(("ANTHROPIC_AUTH_TOKEN".into(), token.to_string()));
        }

        SubprocessSpec {
            program: self.claude_bin.clone(),
            args,
            env_remove: vec!["CLAUDECODE".into()],
            env_set,
        }
    }
}

impl Default for ClaudeCliRunner {
    fn default() -> Self {
        ClaudeCliRunnerConfig::default().build()
    }
}

impl Runner for ClaudeCliRunner {
    fn build_extensions(
        &self,
        provider: &crate::config::domain::ProviderConfig,
    ) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(ClaudeInvokeExtensions {
            base_url: provider.base_url.clone(),
            auth_token: provider.auth_token.clone(),
        }))
    }

    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        let spec = self.build_subprocess_spec(prompt, &options, None);
        let mut session = SubprocessSession::spawn(spec, None)?;
        let stdout = session.take_stdout().expect("stdout taken once");
        let mut decoder = ClaudeNdjsonDecoder::new();

        for line in stdout.lines() {
            let line = line.map_err(|e| AilError::RunnerInvocationFailed {
                detail: e.to_string(),
                context: None,
            })?;
            if line.is_empty() {
                continue;
            }
            if let Err(detail) = decoder.feed(&line, None) {
                // JSON parse error — still need to reap the child.
                let _ = session.finish();
                return Err(AilError::RunnerInvocationFailed {
                    detail,
                    context: None,
                });
            }
            if decoder.is_done() {
                break;
            }
        }

        let outcome = session.finish()?;

        if let Some(detail) = decoder.take_error() {
            return Err(AilError::RunnerInvocationFailed {
                detail,
                context: None,
            });
        }

        if !outcome.exit_status.success() {
            let code = outcome
                .exit_status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            let detail = if outcome.stderr.trim().is_empty() {
                format!("Process exited with {code}")
            } else {
                format!("Process exited with {code}: {}", outcome.stderr.trim())
            };
            tracing::error!(stderr = %outcome.stderr.trim(), "claude CLI exited non-zero");
            return Err(AilError::RunnerInvocationFailed {
                detail,
                context: None,
            });
        }

        decoder.finalize()
    }

    /// Streaming variant — parses `assistant` NDJSON events and emits `RunnerEvent`s.
    /// Sends `StreamDelta` for each text content block, `ToolUse`/`ToolResult` for tool turns,
    /// `CostUpdate` from the final `result` event, then `Completed`.
    ///
    /// If `options.cancel_token` is set, a watchdog thread blocks on the token's event listener
    /// (no polling). When `cancel()` is called, the child subprocess is killed and the
    /// invocation returns `RUNNER_CANCELLED`. This is used by CTRL-C and Ctrl+K in the TUI.
    fn invoke_streaming(
        &self,
        prompt: &str,
        options: InvokeOptions,
        tx: mpsc::Sender<RunnerEvent>,
    ) -> Result<RunResult, AilError> {
        // 1. Optional permission HITL bridge.
        //    The listener binds the socket, writes the hook settings file, and blocks until
        //    ready. Its Drop implementation sends the __close__ sentinel, joins the thread,
        //    removes the file, and cleans up the socket — on every exit path.
        let listener = if !self.headless && options.permission_responder.is_some() {
            Some(ClaudePermissionListener::start(
                options.permission_responder.clone().unwrap(),
                tx.clone(),
            )?)
        } else {
            None
        };

        // 2. Build subprocess spec with the hook settings file path (if any).
        let spec = self.build_subprocess_spec(
            prompt,
            &options,
            listener.as_ref().map(|l| l.settings_file()),
        );

        // 3. Spawn subprocess with optional cancellation watchdog.
        let mut session = SubprocessSession::spawn(spec, options.cancel_token.clone())?;
        let stdout = session.take_stdout().expect("stdout taken once");
        let mut decoder = ClaudeNdjsonDecoder::new();

        // 4. Decode the NDJSON stream.
        for line in stdout.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    // IO error reading stdout. Stash it and break so we can still reap the
                    // child cleanly below.
                    decoder
                        .feed(
                            &serde_json::json!({"type": "result", "subtype": "error",
                                           "is_error": true,
                                           "result": e.to_string()})
                            .to_string(),
                            None,
                        )
                        .ok();
                    break;
                }
            };
            if line.is_empty() {
                continue;
            }
            decoder.feed(&line, Some(&tx)).ok();
            if decoder.is_done() {
                break;
            }
        }

        // 5. Reap the subprocess (stops watchdog, waits child, collects stderr).
        let outcome = session.finish()?;
        // listener is dropped here (if Some) → __close__ sentinel, join, cleanup.

        if outcome.was_cancelled {
            tracing::info!("runner invocation cancelled by user");
            let _ = tx.send(RunnerEvent::Error("cancelled".to_string()));
            return Err(AilError::RunnerCancelled {
                detail: "Runner subprocess was cancelled by user request".to_string(),
                context: None,
            });
        }

        if let Some(detail) = decoder.take_error() {
            let _ = tx.send(RunnerEvent::Error(detail.clone()));
            return Err(AilError::RunnerInvocationFailed {
                detail,
                context: None,
            });
        }

        if !outcome.exit_status.success() {
            let code = outcome
                .exit_status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            let detail = if outcome.stderr.trim().is_empty() {
                format!("Process exited with {code}")
            } else {
                format!("Process exited with {code}: {}", outcome.stderr.trim())
            };
            tracing::error!(stderr = %outcome.stderr.trim(), "claude CLI exited non-zero");
            let _ = tx.send(RunnerEvent::Error(detail.clone()));
            return Err(AilError::RunnerInvocationFailed {
                detail,
                context: None,
            });
        }

        let result = decoder.finalize()?;
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
