use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;

use super::log_provider::{cwd_hash, JsonlProvider, LogProvider};
use crate::config::domain::SamplingConfig;
use crate::runner::{RunResult, ToolEvent};

/// Sampling snapshot recorded in a `TurnEntry` (SPEC §30.8). Mirrors
/// `SamplingConfig` field-for-field as a serializable projection — kept in
/// `turn_log.rs` so `domain.rs` stays free of serde derives.
///
/// Only fields that were actually set are included in the JSON output.
#[derive(Serialize, Debug, Clone, Default)]
pub struct TurnEntrySampling {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<f64>,
}

impl From<&SamplingConfig> for TurnEntrySampling {
    fn from(src: &SamplingConfig) -> Self {
        TurnEntrySampling {
            temperature: src.temperature,
            top_p: src.top_p,
            top_k: src.top_k,
            max_tokens: src.max_tokens,
            stop_sequences: src.stop_sequences.clone(),
            thinking: src.thinking,
        }
    }
}

#[derive(Serialize)]
pub struct TurnEntry {
    pub step_id: String,
    pub prompt: String,
    pub response: Option<String>,
    #[serde(skip)]
    pub timestamp: SystemTime,
    pub cost_usd: Option<f64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// The claude CLI session_id returned by this invocation.
    /// Used to resume the conversation for the next pipeline step.
    pub runner_session_id: Option<String>,
    /// stdout from a `context: shell:` step.
    pub stdout: Option<String>,
    /// stderr from a `context: shell:` step.
    pub stderr: Option<String>,
    /// Exit code from a `context: shell:` step.
    pub exit_code: Option<i32>,
    /// Concatenated thinking/reasoning text from extended thinking blocks, if any.
    /// `None` when no thinking blocks were present (non-prompt steps, or model without thinking).
    pub thinking: Option<String>,
    /// Ordered list of tool call and tool result events captured during this step.
    /// Empty for context:shell steps, sub-pipeline steps, and action steps.
    pub tool_events: Vec<ToolEvent>,
    /// Human-modified output from a HITL `modify_output` gate (SPEC §13.2).
    /// `None` for steps that are not modify gates or when no modification was made.
    pub modified: Option<String>,
    /// Number of iterations completed by a `do_while:` loop step (SPEC §27).
    /// `None` for non-loop steps. 0-based count: a loop that exits after running
    /// its body once has `index: Some(1)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u64>,
    /// Concurrent group ID shared by all steps that were in-flight simultaneously (SPEC §29.8).
    /// `None` for sequential steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrent_group: Option<String>,
    /// Wall-clock timestamp when the step was launched (SPEC §29.8). ISO 8601 format.
    /// Populated for async steps; `None` for sequential steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launched_at: Option<String>,
    /// Wall-clock timestamp when the step completed (SPEC §29.8). ISO 8601 format.
    /// Populated for async steps; `None` for sequential steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// Effective sampling parameters applied to this step (SPEC §30.8).
    /// Only the fields that were actually set at some scope appear in the JSON
    /// output — everything inside the `TurnEntrySampling` struct uses
    /// `skip_serializing_if` so the per-run log stays compact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<TurnEntrySampling>,
}

impl Default for TurnEntry {
    fn default() -> Self {
        TurnEntry {
            step_id: String::new(),
            prompt: String::new(),
            response: None,
            timestamp: SystemTime::now(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            runner_session_id: None,
            stdout: None,
            stderr: None,
            exit_code: None,
            thinking: None,
            tool_events: vec![],
            modified: None,
            index: None,
            concurrent_group: None,
            launched_at: None,
            completed_at: None,
            sampling: None,
        }
    }
}

/// Written as the first entry for a run. Carries pipeline_source and project_hash so the SQLite provider
/// can populate the sessions table correctly.
#[derive(Serialize)]
struct RunStartedEvent<'a> {
    #[serde(rename = "type")]
    event_type: &'static str,
    pipeline_source: Option<&'a str>,
    project_hash: String,
}

/// Written to NDJSON before calling the runner. If the runner crashes or hangs,
/// this record is the only evidence the step was attempted.
#[derive(Serialize)]
struct StepStartedEvent<'a> {
    #[serde(rename = "type")]
    event_type: &'static str,
    step_id: &'a str,
    prompt: &'a str,
}

/// Written when a step fails and `on_error` handling applies (continue or retry).
/// Records the error details and the action taken.
#[derive(Serialize)]
struct StepErrorEvent<'a> {
    #[serde(rename = "type")]
    event_type: &'static str,
    step_id: &'a str,
    error_type: &'a str,
    error_detail: &'a str,
    /// The on_error action taken: "continue", "retry", or "abort_pipeline".
    on_error_action: &'a str,
    /// Current retry attempt (1-based). Only meaningful when on_error_action is "retry".
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_attempt: Option<u32>,
    /// Maximum retries allowed. Only meaningful when on_error_action is "retry".
    #[serde(skip_serializing_if = "Option::is_none")]
    max_retries: Option<u32>,
}

/// Written when an async step is cancelled by a sibling's failure (SPEC §29.8).
#[derive(Serialize)]
struct StepCancelledEvent<'a> {
    #[serde(rename = "type")]
    event_type: &'static str,
    step_id: &'a str,
    cancelled_by: &'a str,
    reason: &'a str,
}

impl TurnEntry {
    /// Record the effective sampling parameters for this turn (SPEC §30.8).
    /// Builder-style so callers can chain it onto `from_prompt` /
    /// `from_context` etc. without enumerating every field.
    pub fn with_sampling(mut self, sampling: Option<&SamplingConfig>) -> Self {
        self.sampling = sampling.map(TurnEntrySampling::from);
        self
    }

    /// Construct a TurnEntry for a completed prompt step.
    pub fn from_prompt(step_id: impl Into<String>, prompt: String, result: RunResult) -> Self {
        TurnEntry {
            step_id: step_id.into(),
            prompt,
            response: Some(result.response),
            cost_usd: result.cost_usd,
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            runner_session_id: result.session_id,
            thinking: result.thinking,
            tool_events: result.tool_events,
            ..Default::default()
        }
    }

    /// Construct a TurnEntry for a completed context:shell: step.
    pub fn from_context(
        step_id: impl Into<String>,
        cmd: String,
        stdout: String,
        stderr: String,
        exit_code: i32,
    ) -> Self {
        TurnEntry {
            step_id: step_id.into(),
            prompt: cmd,
            stdout: Some(stdout),
            stderr: Some(stderr),
            exit_code: Some(exit_code),
            ..Default::default()
        }
    }

    /// Construct a TurnEntry for a `modify_output` HITL gate (SPEC §13.2).
    pub fn from_modify(
        step_id: impl Into<String>,
        message: String,
        modified_output: String,
    ) -> Self {
        TurnEntry {
            step_id: step_id.into(),
            prompt: message,
            modified: Some(modified_output),
            ..Default::default()
        }
    }
}

pub struct TurnLog {
    entries: Vec<TurnEntry>,
    run_id: String,
    provider: Box<dyn LogProvider>,
}

impl TurnLog {
    /// Create a `TurnLog` backed by the default `JsonlProvider` (NDJSON on disk).
    pub fn new(run_id: String) -> Self {
        TurnLog::with_provider(run_id, Box::new(JsonlProvider::new()))
    }

    /// Create a `TurnLog` with an injected `LogProvider`. Useful for tests.
    pub fn with_provider(run_id: String, provider: Box<dyn LogProvider>) -> Self {
        TurnLog {
            entries: Vec::new(),
            run_id,
            provider,
        }
    }

    /// Write a `run_started` event as the first entry. Must be called before any steps.
    /// Carries `pipeline_source` and `project_hash` so the SQLite provider can populate the sessions table.
    pub fn record_run_started(&mut self, pipeline_source: Option<&str>) {
        let project_hash = cwd_hash();

        let event = RunStartedEvent {
            event_type: "run_started",
            pipeline_source,
            project_hash,
        };
        match serde_json::to_value(&event) {
            Ok(json_value) => {
                if let Err(e) = self.provider.write_entry(&self.run_id, &json_value) {
                    tracing::warn!(
                        run_id = %self.run_id,
                        error = %e,
                        "failed to persist run_started event"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    run_id = %self.run_id,
                    error = %e,
                    "failed to serialize run_started event"
                );
            }
        }
    }

    /// Mark the run as finished. Delegates to the provider's `finish()` method.
    pub fn record_run_finished(&mut self, status: &str) {
        if let Err(e) = self.provider.finish(&self.run_id, status) {
            tracing::warn!(
                run_id = %self.run_id,
                status = %status,
                error = %e,
                "failed to finish run in provider"
            );
        }
    }

    /// Full path to the NDJSON run log file.
    /// Delegates to the standalone `log_provider::run_path` helper (always uses the default
    /// project-dir computation regardless of the active provider).
    pub fn run_path(&self) -> PathBuf {
        super::log_provider::run_path(&self.run_id)
    }

    /// Write a `step_started` event to the provider before invoking the runner.
    /// Not added to the in-memory entries — only persisted for observability.
    pub fn record_step_started(&mut self, step_id: &str, prompt: &str) {
        let event = StepStartedEvent {
            event_type: "step_started",
            step_id,
            prompt,
        };
        match serde_json::to_value(&event) {
            Ok(json_value) => {
                if let Err(e) = self.provider.write_entry(&self.run_id, &json_value) {
                    tracing::warn!(
                        run_id = %self.run_id,
                        step_id = %step_id,
                        error = %e,
                        "failed to persist step_started event"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    run_id = %self.run_id,
                    step_id = %step_id,
                    error = %e,
                    "failed to serialize step_started event"
                );
            }
        }
    }

    /// Write a `step_error` event when `on_error` handling fires.
    /// Not added to in-memory entries — persisted for observability only.
    pub fn record_step_error(
        &mut self,
        step_id: &str,
        error_type: &str,
        error_detail: &str,
        on_error_action: &str,
        retry_attempt: Option<u32>,
        max_retries: Option<u32>,
    ) {
        let event = StepErrorEvent {
            event_type: "step_error",
            step_id,
            error_type,
            error_detail,
            on_error_action,
            retry_attempt,
            max_retries,
        };
        match serde_json::to_value(&event) {
            Ok(json_value) => {
                if let Err(e) = self.provider.write_entry(&self.run_id, &json_value) {
                    tracing::warn!(
                        run_id = %self.run_id,
                        step_id = %step_id,
                        error = %e,
                        "failed to persist step_error event"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    run_id = %self.run_id,
                    step_id = %step_id,
                    error = %e,
                    "failed to serialize step_error event"
                );
            }
        }
    }

    /// Write a `step_cancelled` event when a parallel branch is cancelled (SPEC §29.8).
    /// Not added to in-memory entries — persisted for observability only.
    pub fn record_step_cancelled(&mut self, step_id: &str, cancelled_by: &str, reason: &str) {
        let event = StepCancelledEvent {
            event_type: "step_cancelled",
            step_id,
            cancelled_by,
            reason,
        };
        match serde_json::to_value(&event) {
            Ok(json_value) => {
                if let Err(e) = self.provider.write_entry(&self.run_id, &json_value) {
                    tracing::warn!(
                        run_id = %self.run_id,
                        step_id = %step_id,
                        error = %e,
                        "failed to persist step_cancelled event"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    run_id = %self.run_id,
                    step_id = %step_id,
                    error = %e,
                    "failed to serialize step_cancelled event"
                );
            }
        }
    }

    pub fn append(&mut self, entry: TurnEntry) {
        tracing::info!(
            run_id = %self.run_id,
            step_id = %entry.step_id,
            cost_usd = ?entry.cost_usd,
            "turn_log_append"
        );

        match serde_json::to_value(&entry) {
            Ok(json_value) => {
                if let Err(e) = self.provider.write_entry(&self.run_id, &json_value) {
                    tracing::warn!(run_id = %self.run_id, error = %e, "failed to persist turn log entry");
                }
            }
            Err(e) => {
                tracing::warn!(run_id = %self.run_id, error = %e, "failed to serialize turn log entry");
            }
        }

        self.entries.push(entry);
    }

    pub fn last_response(&self) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find_map(|e| e.response.as_deref())
    }

    pub fn last_runner_session_id(&self) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find_map(|e| e.runner_session_id.as_deref())
    }

    pub fn response_for_step(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .and_then(|e| e.response.as_deref())
    }

    /// Combined stdout + stderr for a context step, or the response for a prompt step.
    /// Returns `None` if no entry exists for `id`.
    pub fn result_for_step(&self, id: &str) -> Option<String> {
        let entry = self.entries.iter().find(|e| e.step_id == id)?;
        if entry.stdout.is_some() || entry.stderr.is_some() {
            let stdout = entry.stdout.as_deref().unwrap_or("");
            let stderr = entry.stderr.as_deref().unwrap_or("");
            if stderr.is_empty() {
                Some(stdout.to_string())
            } else if stdout.is_empty() {
                Some(stderr.to_string())
            } else {
                Some(format!("{stdout}\n{stderr}"))
            }
        } else {
            entry.response.clone()
        }
    }

    pub fn stdout_for_step(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .and_then(|e| e.stdout.as_deref())
    }

    pub fn stderr_for_step(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .and_then(|e| e.stderr.as_deref())
    }

    pub fn exit_code_for_step(&self, id: &str) -> Option<i32> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .and_then(|e| e.exit_code)
    }

    /// Tool events recorded for a step (empty slice when step has no tool calls).
    pub fn tool_events_for_step(&self, id: &str) -> Option<&[ToolEvent]> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .map(|e| e.tool_events.as_slice())
    }

    /// Human-modified output for a `modify_output` HITL gate step (SPEC §13.2).
    pub fn modified_for_step(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .and_then(|e| e.modified.as_deref())
    }

    /// Number of iterations completed by a `do_while:` loop step (SPEC §27).
    /// Returns `None` for non-loop steps or if no entry exists.
    pub fn index_for_step(&self, id: &str) -> Option<u64> {
        self.entries
            .iter()
            .find(|e| e.step_id == id)
            .and_then(|e| e.index)
    }

    /// Remove in-memory entries whose `step_id` starts with `prefix`.
    ///
    /// Used by `do_while:` execution to clear the previous iteration's inner step
    /// results before starting a new iteration (SPEC §27.3 — iteration scope).
    /// Already-persisted entries are unaffected; this only affects template variable
    /// resolution which reads from the in-memory store.
    pub fn remove_entries_with_prefix(&mut self, prefix: &str) {
        self.entries.retain(|e| !e.step_id.starts_with(prefix));
    }

    pub fn entries(&self) -> &[TurnEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::ToolEvent;
    use crate::session::log_provider::NullProvider;
    use std::time::SystemTime;

    fn make_entry(step_id: &str, response: Option<&str>) -> TurnEntry {
        TurnEntry {
            step_id: step_id.to_string(),
            prompt: "prompt".to_string(),
            response: response.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    fn null_log() -> TurnLog {
        TurnLog::with_provider("test-run".to_string(), Box::new(NullProvider))
    }

    #[test]
    fn new_with_null_provider_has_empty_entries() {
        let log = null_log();
        assert!(log.entries().is_empty());
    }

    #[test]
    fn append_entry_is_accessible_via_entries() {
        let mut log = null_log();
        log.append(make_entry("step-1", Some("response")));
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].step_id, "step-1");
    }

    #[test]
    fn last_response_is_none_when_empty() {
        let log = null_log();
        assert!(log.last_response().is_none());
    }

    #[test]
    fn last_response_returns_response_of_single_entry() {
        let mut log = null_log();
        log.append(make_entry("step-1", Some("hello")));
        assert_eq!(log.last_response(), Some("hello"));
    }

    #[test]
    fn last_response_returns_last_entry_response_with_multiple_entries() {
        let mut log = null_log();
        log.append(make_entry("step-1", Some("first")));
        log.append(make_entry("step-2", Some("second")));
        assert_eq!(log.last_response(), Some("second"));
    }

    #[test]
    fn record_run_started_does_not_panic() {
        let mut log = null_log();
        log.record_run_started(Some("test.ail.yaml"));
        log.record_run_started(None);
    }

    #[test]
    fn record_step_started_does_not_panic() {
        let mut log = null_log();
        log.record_step_started("step-1", "do something");
    }

    #[test]
    fn last_response_skips_entries_with_no_response() {
        let mut log = null_log();
        log.append(make_entry("step-1", Some("has response")));
        // Entry with no response — last_response should skip it and return the previous one
        log.append(make_entry("step-2", None));
        assert_eq!(log.last_response(), Some("has response"));
    }
}
