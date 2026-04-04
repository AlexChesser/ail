use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;
use sha1::{Digest, Sha1};

use super::log_provider::{JsonlProvider, LogProvider};

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
        // Compute project_hash from the current working directory.
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut hasher = Sha1::new();
        hasher.update(cwd.to_string_lossy().as_bytes());
        let project_hash = format!("{:x}", hasher.finalize());

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

    pub fn entries(&self) -> &[TurnEntry] {
        &self.entries
    }
}
