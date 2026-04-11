//! Direct HTTP runner — calls any OpenAI-compatible `/v1/chat/completions` endpoint.
//!
//! Unlike [`super::claude`], this runner calls the provider API directly without
//! wrapping Claude CLI, giving full control over the system prompt and model parameters.
//! Works with Ollama, direct Anthropic API, and any OpenAI-compatible provider.
//!
//! Session continuity is maintained in memory: the first call returns a UUID session ID;
//! subsequent pipeline steps pass that ID as `resume_session_id` to replay the full
//! message history in the next API call.

#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AilError;
use crate::runner::{InvokeOptions, RunResult, Runner, ToolPermissionPolicy};

// ── Wire DTOs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    model: Option<String>,
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

// ── Config ─────────────────────────────────────────────────────────────────────

/// Configuration for [`HttpRunner`].
#[derive(Debug, Clone)]
pub struct HttpRunnerConfig {
    /// Base URL for the OpenAI-compatible API, e.g. `"http://localhost:11434/v1"`.
    pub base_url: String,
    /// Authentication token. Sent as `Authorization: Bearer {token}` when set.
    pub auth_token: Option<String>,
    /// Default model name (used when `InvokeOptions.model` is absent).
    pub default_model: Option<String>,
    /// When `Some(false)`, sends `"think": false` in the request body — disables
    /// Ollama's extended thinking mode for models like qwen3 that default to it.
    /// Leave as `None` to omit the field and let the server decide.
    pub think: Option<bool>,
}

impl Default for HttpRunnerConfig {
    fn default() -> Self {
        HttpRunnerConfig {
            base_url: "http://localhost:11434/v1".to_string(),
            auth_token: None,
            default_model: None,
            think: None,
        }
    }
}

// ── Runner ─────────────────────────────────────────────────────────────────────

/// Direct HTTP runner for OpenAI-compatible endpoints.
///
/// Use [`HttpRunner::new`] with a custom config or [`HttpRunner::ollama`] for a
/// local Ollama instance with thinking disabled.
pub struct HttpRunner {
    config: HttpRunnerConfig,
    /// In-memory conversation store: session_id → full message history.
    ///
    /// Stored messages: [system?, user, assistant, user, assistant, …]
    /// — always the complete context needed to resume the conversation.
    conversations: Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>,
}

impl HttpRunner {
    pub fn new(config: HttpRunnerConfig) -> Self {
        HttpRunner {
            config,
            conversations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Convenience constructor for a local Ollama instance with thinking disabled.
    ///
    /// Sets `base_url = "http://localhost:11434/v1"` and `think = Some(false)`.
    pub fn ollama(model: impl Into<String>) -> Self {
        Self::new(HttpRunnerConfig {
            default_model: Some(model.into()),
            think: Some(false),
            ..HttpRunnerConfig::default()
        })
    }

    /// Build the system prompt string from `InvokeOptions`.
    fn build_system_prompt(options: &InvokeOptions) -> String {
        let base = options.system_prompt.as_deref().unwrap_or_default();
        if options.append_system_prompt.is_empty() {
            base.to_string()
        } else {
            let mut parts = vec![base.to_string()];
            parts.extend(options.append_system_prompt.iter().cloned());
            parts.join("\n\n")
        }
    }

    /// Map `ureq::Error` to `AilError::RunnerInvocationFailed`.
    fn map_ureq_error(err: ureq::Error) -> AilError {
        let detail = match err {
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_default();
                format!("HTTP {code}: {body}")
            }
            ureq::Error::Transport(t) => format!("Transport error: {t}"),
        };
        AilError::RunnerInvocationFailed {
            detail,
            context: None,
        }
    }
}

impl Runner for HttpRunner {
    fn invoke(&self, prompt: &str, options: InvokeOptions) -> Result<RunResult, AilError> {
        let model = options
            .model
            .as_deref()
            .or(self.config.default_model.as_deref())
            .ok_or_else(|| AilError::RunnerInvocationFailed {
                detail: "HttpRunner: no model specified. Set InvokeOptions.model or \
                         HttpRunnerConfig.default_model"
                    .to_string(),
                context: None,
            })?
            .to_string();

        if !matches!(
            options.tool_policy,
            ToolPermissionPolicy::RunnerDefault | ToolPermissionPolicy::NoTools
        ) {
            tracing::warn!(
                "HttpRunner: tool policies other than RunnerDefault/NoTools are not supported; \
                 ignoring tool_policy"
            );
        }

        // ── Build message history ────────────────────────────────────────────

        let resume_id = options.resume_session_id.as_deref();
        let mut api_messages: Vec<ChatMessage> = Vec::new();

        if let Some(id) = resume_id {
            // Resume: load stored history (which ends with the last assistant turn).
            let store = self.conversations.lock().map_err(|_| {
                AilError::RunnerInvocationFailed {
                    detail: "HttpRunner: conversation store lock poisoned".to_string(),
                    context: None,
                }
            })?;
            if let Some(history) = store.get(id) {
                api_messages.extend_from_slice(history);
            }
        } else {
            // Fresh conversation: prepend system prompt when present.
            let system = Self::build_system_prompt(&options);
            if !system.is_empty() {
                api_messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: system,
                });
            }
        }

        // Append the current user turn.
        api_messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        // ── HTTP call ────────────────────────────────────────────────────────

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        tracing::debug!(
            url = %url,
            model = %model,
            messages = api_messages.len(),
            resume = resume_id.is_some(),
            "HttpRunner: invoking"
        );

        let body = ChatRequest {
            model: &model,
            messages: &api_messages,
            stream: false,
            think: self.config.think,
        };

        let auth_header = self
            .config
            .auth_token
            .as_deref()
            .map(|t| format!("Bearer {t}"));

        let req = ureq::post(&url).set("Content-Type", "application/json");
        let req = match &auth_header {
            Some(auth) => req.set("Authorization", auth),
            None => req,
        };

        let response: ChatResponse = req
            .send_json(ureq::serde_json::json!(body))
            .map_err(Self::map_ureq_error)?
            .into_json()
            .map_err(|e| AilError::RunnerInvocationFailed {
                detail: format!("HttpRunner: failed to parse response JSON: {e}"),
                context: None,
            })?;

        // ── Extract result ───────────────────────────────────────────────────

        let model_used = response.model;
        let content = response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        let (input_tokens, output_tokens) = response
            .usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .unwrap_or((0, 0));

        tracing::debug!(
            input_tokens,
            output_tokens,
            response_len = content.len(),
            "HttpRunner: invocation complete"
        );

        // ── Update conversation history ──────────────────────────────────────

        let session_id = resume_id
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        {
            let mut store = self.conversations.lock().map_err(|_| {
                AilError::RunnerInvocationFailed {
                    detail: "HttpRunner: conversation store lock poisoned".to_string(),
                    context: None,
                }
            })?;
            // Store the full context: what we sent + the assistant response.
            let history = store.entry(session_id.clone()).or_default();
            *history = api_messages;
            history.push(ChatMessage {
                role: "assistant".to_string(),
                content: content.clone(),
            });
        }

        Ok(RunResult {
            response: content,
            cost_usd: None,
            session_id: Some(session_id),
            input_tokens,
            output_tokens,
            thinking: None,
            model: model_used,
            tool_events: vec![],
        })
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_constructor_sets_defaults() {
        let r = HttpRunner::ollama("qwen3:0.6b");
        assert_eq!(r.config.default_model.as_deref(), Some("qwen3:0.6b"));
        assert_eq!(r.config.think, Some(false));
        assert_eq!(r.config.base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn new_with_config_stores_fields() {
        let r = HttpRunner::new(HttpRunnerConfig {
            base_url: "http://api.example.com/v1".to_string(),
            auth_token: Some("tok".to_string()),
            default_model: Some("gpt-4o".to_string()),
            think: None,
        });
        assert_eq!(r.config.base_url, "http://api.example.com/v1");
        assert_eq!(r.config.auth_token.as_deref(), Some("tok"));
        assert_eq!(r.config.default_model.as_deref(), Some("gpt-4o"));
        assert!(r.config.think.is_none());
    }

    #[test]
    fn build_system_prompt_combines_base_and_appended() {
        let opts = InvokeOptions {
            system_prompt: Some("Base instructions.".to_string()),
            append_system_prompt: vec!["Extra rule 1.".to_string(), "Extra rule 2.".to_string()],
            ..InvokeOptions::default()
        };
        let result = HttpRunner::build_system_prompt(&opts);
        assert!(result.contains("Base instructions."));
        assert!(result.contains("Extra rule 1."));
        assert!(result.contains("Extra rule 2."));
    }

    #[test]
    fn build_system_prompt_empty_when_nothing_set() {
        let opts = InvokeOptions::default();
        let result = HttpRunner::build_system_prompt(&opts);
        assert!(result.is_empty());
    }

    #[test]
    fn build_system_prompt_base_only_when_no_append() {
        let opts = InvokeOptions {
            system_prompt: Some("Just the base.".to_string()),
            ..InvokeOptions::default()
        };
        assert_eq!(HttpRunner::build_system_prompt(&opts), "Just the base.");
    }

    #[test]
    fn conversation_store_starts_empty() {
        let r = HttpRunner::new(HttpRunnerConfig::default());
        let store = r.conversations.lock().unwrap();
        assert!(store.is_empty());
    }

    /// Live test — requires a running Ollama instance. Marked #[ignore] by default.
    #[test]
    #[ignore]
    fn live_ollama_invoke_returns_nonempty_response() {
        let runner = HttpRunner::ollama("qwen3:0.6b");
        let options = InvokeOptions {
            model: Some("qwen3:0.6b".to_string()),
            system_prompt: Some("Reply with exactly one word: TRIVIAL.".to_string()),
            ..InvokeOptions::default()
        };
        let result = runner.invoke("Hello", options).unwrap();
        assert!(!result.response.is_empty());
        assert!(result.session_id.is_some());
        println!("Response: {}", result.response);
    }

    /// Live multi-turn test — verifies session continuity across two steps.
    #[test]
    #[ignore]
    fn live_ollama_multi_turn_session_continuity() {
        let runner = HttpRunner::ollama("qwen3:0.6b");

        let first = runner
            .invoke(
                "My favourite colour is blue. Remember that.",
                InvokeOptions {
                    model: Some("qwen3:0.6b".to_string()),
                    ..InvokeOptions::default()
                },
            )
            .unwrap();
        let session_id = first.session_id.clone().unwrap();

        let second = runner
            .invoke(
                "What is my favourite colour?",
                InvokeOptions {
                    model: Some("qwen3:0.6b".to_string()),
                    resume_session_id: Some(session_id),
                    ..InvokeOptions::default()
                },
            )
            .unwrap();

        println!("Second response: {}", second.response);
        assert!(
            second.response.to_lowercase().contains("blue"),
            "Expected 'blue' in response, got: {}",
            second.response
        );
    }
}
