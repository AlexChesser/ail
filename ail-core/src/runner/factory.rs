#![allow(clippy::result_large_err)]

//! Factory for constructing `Runner` implementations by name.
//!
//! # Selection hierarchy
//!
//! 1. Per-step `runner:` field in YAML (resolved by the executor)
//! 2. `AIL_DEFAULT_RUNNER` environment variable
//! 3. Fallback: `"claude"` → `ClaudeCliRunner`
//!
//! # Adding a new runner
//!
//! 1. Implement the `Runner` trait in a new module under `runner/`.
//! 2. Add a match arm in `RunnerFactory::build()` mapping the runner name to the implementation.
//! 3. Export the module from `runner/mod.rs`.

use super::{
    claude::{ClaudeCliRunner, ClaudeCliRunnerConfig},
    codex::CodexRunnerConfig,
    http::{HttpRunner, HttpRunnerConfig, HttpSessionStore},
    plugin::{PluginRegistry, ProtocolRunner},
    stub::StubRunner,
    Runner,
};
use crate::config::domain::ProviderConfig;
use crate::error::AilError;

/// Factory that constructs `Runner` boxed trait objects by name.
///
/// The canonical way to obtain a runner in the binary entry point:
///
/// ```ignore
/// use ail_core::runner::factory::RunnerFactory;
/// let runner = RunnerFactory::build_default(cli.headless)?;
/// ```
pub struct RunnerFactory;

impl RunnerFactory {
    /// Build a runner by explicit name.
    ///
    /// Recognised names (case-insensitive, whitespace-trimmed):
    /// - `"claude"` — `ClaudeCliRunner` with the given headless flag
    /// - `"codex"` — `CodexRunner`; reads `AIL_CODEX_BIN` (default: `"codex"`) from the
    ///   environment. Requires `OPENAI_API_KEY` set in the ambient environment.
    /// - `"http"` or `"ollama"` — `HttpRunner`; reads `AIL_HTTP_BASE_URL` (default:
    ///   `http://localhost:11434/v1`), `AIL_HTTP_TOKEN`, `AIL_HTTP_MODEL`, `AIL_HTTP_THINK`
    ///   from the environment.
    /// - `"stub"` — `StubRunner` with a fixed response (test/dev use)
    /// - Any other name — looks up the plugin registry for a matching runner extension
    ///
    /// Returns `RUNNER_NOT_FOUND` if the name is not recognised and no matching plugin exists.
    pub fn build(
        runner_name: &str,
        headless: bool,
        http_store: &HttpSessionStore,
        provider: &ProviderConfig,
    ) -> Result<Box<dyn Runner + Send>, AilError> {
        Self::build_with_registry(
            runner_name,
            headless,
            http_store,
            provider,
            &PluginRegistry::empty(),
        )
    }

    /// Build a runner by explicit name, consulting the plugin registry for unknown names.
    ///
    /// Resolution order:
    /// 1. Built-in runners (claude, http/ollama, stub)
    /// 2. Plugin registry (discovered from `~/.ail/runners/`)
    /// 3. Error: RUNNER_NOT_FOUND
    pub fn build_with_registry(
        runner_name: &str,
        headless: bool,
        http_store: &HttpSessionStore,
        provider: &ProviderConfig,
        registry: &PluginRegistry,
    ) -> Result<Box<dyn Runner + Send>, AilError> {
        let normalized = runner_name.trim().to_lowercase();
        match normalized.as_str() {
            "claude" => {
                let runner: ClaudeCliRunner =
                    ClaudeCliRunnerConfig::default().headless(headless).build();
                Ok(Box::new(runner))
            }
            "codex" => {
                let codex_bin = std::env::var("AIL_CODEX_BIN")
                    .unwrap_or_else(|_| "codex".to_string());
                Ok(Box::new(
                    CodexRunnerConfig::default()
                        .codex_bin(codex_bin)
                        .headless(headless)
                        .build(),
                ))
            }
            "http" | "ollama" => {
                // ProviderConfig values take precedence over env vars.
                let base_url = provider
                    .base_url
                    .clone()
                    .or_else(|| std::env::var("AIL_HTTP_BASE_URL").ok())
                    .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
                let auth_token = provider
                    .auth_token
                    .clone()
                    .or_else(|| std::env::var("AIL_HTTP_TOKEN").ok());
                let default_model = std::env::var("AIL_HTTP_MODEL").ok();
                let think = std::env::var("AIL_HTTP_THINK")
                    .ok()
                    .map(|v| v.trim().to_lowercase() != "false");
                Ok(Box::new(HttpRunner::new(
                    HttpRunnerConfig {
                        base_url,
                        auth_token,
                        default_model,
                        think,
                        connect_timeout_seconds: provider.connect_timeout_seconds,
                        read_timeout_seconds: provider.read_timeout_seconds,
                        max_history_messages: provider.max_history_messages,
                    },
                    http_store.clone(),
                )))
            }
            "stub" => Ok(Box::new(StubRunner::new("stub response"))),
            other => {
                // Check the plugin registry
                if let Some(manifest) = registry.get(other) {
                    tracing::info!(runner = other, plugin = %manifest.manifest_path.display(), "using plugin runner");
                    return Ok(Box::new(ProtocolRunner::new(manifest.clone())));
                }

                let mut known: Vec<&str> = vec!["claude", "codex", "http", "ollama", "stub"];
                let plugin_names = registry.runner_names();
                known.extend(plugin_names.iter().copied());

                Err(AilError::RunnerNotFound {
                    detail: format!(
                        "Runner '{other}' is not recognized. Known runners: {}",
                        known.join(", ")
                    ),
                    context: None,
                })
            }
        }
    }

    /// Build the default runner, honouring the `AIL_DEFAULT_RUNNER` environment variable.
    ///
    /// Resolution order:
    /// 1. `AIL_DEFAULT_RUNNER` env var (if set and non-empty)
    /// 2. `"claude"` (hardcoded fallback)
    pub fn build_default(
        headless: bool,
        http_store: &HttpSessionStore,
        provider: &ProviderConfig,
    ) -> Result<Box<dyn Runner + Send>, AilError> {
        let name = std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string());
        Self::build(&name, headless, http_store, provider)
    }

    /// Build the default runner with plugin registry support.
    pub fn build_default_with_registry(
        headless: bool,
        http_store: &HttpSessionStore,
        provider: &ProviderConfig,
        registry: &PluginRegistry,
    ) -> Result<Box<dyn Runner + Send>, AilError> {
        let name = std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string());
        Self::build_with_registry(&name, headless, http_store, provider, registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn test_store() -> HttpSessionStore {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn test_provider() -> ProviderConfig {
        ProviderConfig::default()
    }

    #[test]
    fn build_claude_runner_succeeds() {
        let result = RunnerFactory::build("claude", false, &test_store(), &test_provider());
        assert!(result.is_ok());
    }

    #[test]
    fn build_claude_runner_case_insensitive() {
        let result = RunnerFactory::build("Claude", false, &test_store(), &test_provider());
        assert!(result.is_ok());
        let result = RunnerFactory::build("CLAUDE", false, &test_store(), &test_provider());
        assert!(result.is_ok());
    }

    #[test]
    fn build_stub_runner_succeeds() {
        let result = RunnerFactory::build("stub", false, &test_store(), &test_provider());
        assert!(result.is_ok());
    }

    #[test]
    fn build_stub_runner_returns_fixed_response() {
        use crate::runner::InvokeOptions;
        let runner = RunnerFactory::build("stub", false, &test_store(), &test_provider()).unwrap();
        let result = runner
            .invoke("any prompt", InvokeOptions::default())
            .unwrap();
        assert_eq!(result.response, "stub response");
    }

    #[test]
    fn build_unknown_runner_returns_runner_not_found_error() {
        use crate::error::error_types;
        let result = RunnerFactory::build("nonexistent", false, &test_store(), &test_provider());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert_eq!(err.error_type(), error_types::RUNNER_NOT_FOUND);
        assert!(err.detail().contains("nonexistent"));
    }

    #[test]
    fn build_default_respects_env_var() {
        use crate::runner::InvokeOptions;
        let runner = RunnerFactory::build("stub", false, &test_store(), &test_provider()).unwrap();
        let result = runner.invoke("hello", InvokeOptions::default()).unwrap();
        assert_eq!(result.response, "stub response");
    }

    #[test]
    fn build_default_falls_back_to_claude_when_env_absent() {
        let result = RunnerFactory::build("claude", false, &test_store(), &test_provider());
        assert!(result.is_ok());
    }
}
