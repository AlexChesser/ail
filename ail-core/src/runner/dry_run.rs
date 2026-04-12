//! Dry-run runner — captures what would be sent without making LLM API calls.
//!
//! Used by `--dry-run` mode to show the full pipeline resolution (template variable
//! substitution, step ordering, resolved prompts) without invoking any LLM provider.
//! Shell context steps are always executed since they are local and free.

use super::{InvokeOptions, RunResult, Runner};
use crate::error::AilError;

/// A runner that returns a synthetic response without calling any LLM API.
///
/// Each invocation records the prompt and step metadata, and returns a fixed
/// `[DRY RUN]` response so template variables referencing earlier step responses
/// remain resolvable for subsequent steps.
pub struct DryRunRunner;

impl DryRunRunner {
    pub fn new() -> Self {
        DryRunRunner
    }
}

impl Default for DryRunRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner for DryRunRunner {
    fn invoke(&self, prompt: &str, _options: InvokeOptions) -> Result<RunResult, AilError> {
        tracing::info!(
            mode = "dry_run",
            prompt_len = prompt.len(),
            "dry-run runner: skipping LLM invocation"
        );
        Ok(RunResult {
            response: "[DRY RUN] No LLM call made".to_string(),
            cost_usd: Some(0.0),
            session_id: Some("dry-run-session".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            thinking: None,
            model: None,
            tool_events: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_runner_returns_synthetic_response() {
        let runner = DryRunRunner::new();
        let result = runner
            .invoke("any prompt", InvokeOptions::default())
            .unwrap();
        assert!(result.response.contains("[DRY RUN]"));
        assert_eq!(result.cost_usd, Some(0.0));
        assert_eq!(result.input_tokens, 0);
        assert_eq!(result.output_tokens, 0);
    }

    #[test]
    fn dry_run_runner_ignores_prompt_content() {
        let runner = DryRunRunner::new();
        let r1 = runner
            .invoke("prompt one", InvokeOptions::default())
            .unwrap();
        let r2 = runner
            .invoke("prompt two", InvokeOptions::default())
            .unwrap();
        assert_eq!(r1.response, r2.response);
    }

    #[test]
    fn dry_run_runner_default_trait() {
        let runner = DryRunRunner::default();
        let result = runner.invoke("test", InvokeOptions::default()).unwrap();
        assert!(result.response.contains("[DRY RUN]"));
    }
}
