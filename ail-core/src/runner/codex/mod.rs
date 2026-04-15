//! `CodexRunner` — drives `codex exec --json`.
//!
//! Wraps the OpenAI Codex CLI (`@openai/codex`, installed via `npm i -g @openai/codex`)
//! in non-interactive mode. The CLI must be available in PATH or configured via the
//! `AIL_CODEX_BIN` environment variable.
//!
//! `OPENAI_API_KEY` is read from the environment by the `codex` binary itself and is
//! **not** set or modified by this runner.
//!
//! ## Unsupported `InvokeOptions` fields
//!
//! The Codex CLI does not expose a hook mechanism analogous to Claude's MCP bridge, and
//! uses sandbox levels rather than named tool allowlists. The following fields are silently
//! ignored (with a `warn!` log for tool policy, `trace!` for others):
//!
//! - `tool_policy` — logged at `WARN` level; Codex sandbox level is fixed by `--full-auto`
//! - `system_prompt` / `append_system_prompt` — no CLI equivalent
//! - `permission_responder` — no hook mechanism in the Codex CLI
//!
//! See `spec/runner/r06-codex-runner.md` for the full contract.

#![allow(clippy::result_large_err)]

pub mod decoder;
mod wire_dto;

use std::io::BufRead;
use std::sync::mpsc;

use decoder::CodexNdjsonDecoder;

use super::subprocess::{SubprocessSession, SubprocessSpec};
use super::{InvokeOptions, RunResult, Runner, RunnerEvent, ToolPermissionPolicy};
use crate::error::AilError;

// ── Config and runner struct ──────────────────────────────────────────────────────────────────

/// Builder for [`CodexRunner`]. Encapsulates all Codex-specific construction parameters.
///
/// # Example
/// ```ignore
/// let runner = CodexRunnerConfig::default().headless(true).build();
/// ```
#[derive(Debug, Clone)]
pub struct CodexRunnerConfig {
    pub codex_bin: String,
    /// When true, passes `--full-auto` to the codex CLI, enabling automated tool execution
    /// without per-call confirmation. Equivalent to Claude's `--dangerously-skip-permissions`.
    pub headless: bool,
}

impl Default for CodexRunnerConfig {
    fn default() -> Self {
        Self {
            codex_bin: "codex".to_string(),
            headless: false,
        }
    }
}

impl CodexRunnerConfig {
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    pub fn codex_bin(mut self, bin: impl Into<String>) -> Self {
        self.codex_bin = bin.into();
        self
    }

    pub fn build(self) -> CodexRunner {
        CodexRunner {
            codex_bin: self.codex_bin,
            headless: self.headless,
        }
    }
}

/// Drives the Codex CLI in `codex exec --json` mode.
///
/// Construct via [`CodexRunnerConfig`]:
/// ```ignore
/// let runner = CodexRunnerConfig::default().headless(true).build();
/// ```
pub struct CodexRunner {
    pub codex_bin: String,
    /// When true, passes `--full-auto` to the codex CLI (automated sandbox execution).
    pub headless: bool,
}

impl CodexRunner {
    pub fn from_config(config: CodexRunnerConfig) -> Self {
        config.build()
    }

    // ── Private helpers ───────────────────────────────────────────────────────────────────────

    /// Translate `InvokeOptions` into a [`SubprocessSpec`] ready to pass to
    /// [`SubprocessSession::spawn`].
    ///
    /// Unsupported options are logged and ignored — see module-level doc for the full list.
    fn build_subprocess_spec(&self, prompt: &str, options: &InvokeOptions) -> SubprocessSpec {
        // Log unsupported options so operators can diagnose unexpected behaviour.
        match &options.tool_policy {
            ToolPermissionPolicy::RunnerDefault => {}
            other => {
                tracing::warn!(
                    policy = ?other,
                    "CodexRunner: tool_policy is not supported by the codex CLI; \
                     the runner's default sandbox level will be used"
                );
            }
        }
        if options.system_prompt.is_some() || !options.append_system_prompt.is_empty() {
            tracing::trace!(
                "CodexRunner: system_prompt / append_system_prompt are not supported \
                 by the codex CLI; ignoring"
            );
        }
        if options.permission_responder.is_some() {
            tracing::trace!(
                "CodexRunner: permission_responder is not supported by the codex CLI; ignoring"
            );
        }
        // SPEC §30.4.1 — warn-and-ignore unsupported sampling fields so pipelines
        // remain portable. The codex CLI exposes no sampling flags today.
        if let Some(s) = options.sampling.as_ref() {
            if s.temperature.is_some()
                || s.top_p.is_some()
                || s.top_k.is_some()
                || s.max_tokens.is_some()
                || s.stop_sequences.is_some()
                || s.thinking.is_some()
            {
                tracing::warn!(
                    "CodexRunner: sampling parameters are not supported by the codex \
                     CLI; ignoring (temperature/top_p/top_k/max_tokens/stop_sequences/thinking)"
                );
            }
        }

        // Build the argument list.
        // Invocation form:
        //   codex exec [resume <thread_id>] --json [--model <model>] [--full-auto] <prompt>
        let mut args: Vec<String> = vec!["exec".into()];

        if let Some(ref sid) = options.resume_session_id {
            args.push("resume".into());
            args.push(sid.clone());
        }

        args.push("--json".into());

        if let Some(ref model) = options.model {
            args.push("--model".into());
            args.push(model.clone());
        }

        if self.headless {
            args.push("--full-auto".into());
        }

        args.push(prompt.to_string());

        SubprocessSpec {
            program: self.codex_bin.clone(),
            args,
            // No nested-session guard equivalent for codex.
            env_remove: vec![],
            // OPENAI_API_KEY is read by the codex binary from the ambient environment;
            // this runner does not set or modify it.
            env_set: vec![],
        }
    }
}

impl Default for CodexRunner {
    fn default() -> Self {
        CodexRunnerConfig::default().build()
    }
}

impl Runner for CodexRunner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        let spec = self.build_subprocess_spec(prompt, &options);
        let mut session = SubprocessSession::spawn(spec, None)?;
        let stdout = session.take_stdout().expect("stdout taken once");
        let mut decoder = CodexNdjsonDecoder::new();

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
            tracing::error!(stderr = %outcome.stderr.trim(), "codex CLI exited non-zero");
            return Err(AilError::RunnerInvocationFailed {
                detail,
                context: None,
            });
        }

        decoder.finalize()
    }

    /// Streaming variant — parses Codex NDJSON events and emits `RunnerEvent`s.
    ///
    /// If `options.cancel_token` is set, a watchdog thread blocks on the token's event
    /// listener (no polling). When `cancel()` is called, the child subprocess is killed and
    /// the invocation returns `RUNNER_CANCELLED`.
    fn invoke_streaming(
        &self,
        prompt: &str,
        options: InvokeOptions,
        tx: mpsc::Sender<RunnerEvent>,
    ) -> Result<RunResult, AilError> {
        let spec = self.build_subprocess_spec(prompt, &options);
        let mut session = SubprocessSession::spawn(spec, options.cancel_token.clone())?;
        let stdout = session.take_stdout().expect("stdout taken once");
        let mut decoder = CodexNdjsonDecoder::new();

        for line in stdout.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    // IO error reading stdout — stash it and break to reap child cleanly.
                    decoder
                        .feed(
                            &serde_json::json!({
                                "type": "error",
                                "message": e.to_string()
                            })
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

        let outcome = session.finish()?;

        if outcome.was_cancelled {
            tracing::info!("codex runner invocation cancelled by user");
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
            tracing::error!(stderr = %outcome.stderr.trim(), "codex CLI exited non-zero");
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

    /// Requires: `codex` CLI in PATH, `OPENAI_API_KEY` set, run outside a Claude Code
    /// session.
    ///
    /// Run with: cargo nextest run -- --ignored
    #[test]
    #[ignore]
    fn codex_runner_returns_non_empty_response() {
        let runner = CodexRunnerConfig::default().build();
        let result = runner
            .invoke(
                "Reply with exactly the word: hello",
                InvokeOptions::default(),
            )
            .unwrap();
        assert!(!result.response.is_empty());
        assert!(result.session_id.is_some());
    }

    #[test]
    #[ignore]
    fn codex_runner_response_contains_expected_text() {
        let runner = CodexRunnerConfig::default().build();
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
