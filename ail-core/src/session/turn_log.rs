use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;
use sha1::{Digest, Sha1};

#[derive(Serialize)]
pub struct TurnEntry {
    pub step_id: String,
    pub prompt: String,
    pub response: Option<String>,
    #[serde(skip)]
    pub timestamp: SystemTime,
    pub cost_usd: Option<f64>,
    /// The claude CLI session_id returned by this invocation.
    /// Used to resume the conversation for the next pipeline step.
    pub runner_session_id: Option<String>,
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
    /// `~/.ail/projects/<sha1_of_cwd>` — deterministic per working directory.
    project_dir: PathBuf,
}

impl TurnLog {
    pub fn new(run_id: String) -> Self {
        TurnLog {
            entries: Vec::new(),
            run_id,
            project_dir: project_dir(),
        }
    }

    /// Full path to the NDJSON run log file.
    pub fn run_path(&self) -> PathBuf {
        self.project_dir
            .join("runs")
            .join(format!("{}.jsonl", self.run_id))
    }

    /// Write a `step_started` event to NDJSON before invoking the runner.
    /// Not added to the in-memory entries — only persisted for observability.
    pub fn record_step_started(&self, step_id: &str, prompt: &str) {
        let event = StepStartedEvent {
            event_type: "step_started",
            step_id,
            prompt,
        };
        if let Err(e) = self.write_ndjson(&event) {
            tracing::warn!(
                run_id = %self.run_id,
                step_id = %step_id,
                error = %e,
                "failed to persist step_started event"
            );
        }
    }

    pub fn append(&mut self, entry: TurnEntry) {
        tracing::info!(
            run_id = %self.run_id,
            step_id = %entry.step_id,
            cost_usd = ?entry.cost_usd,
            "turn_log_append"
        );

        if let Err(e) = self.write_ndjson(&entry) {
            tracing::warn!(run_id = %self.run_id, error = %e, "failed to persist turn log entry");
        }

        self.entries.push(entry);
    }

    fn write_ndjson<T: Serialize>(&self, value: &T) -> std::io::Result<()> {
        let dir = self.project_dir.join("runs");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.jsonl", self.run_id));
        let line = serde_json::to_string(value).map_err(std::io::Error::other)?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{line}")
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

    pub fn entries(&self) -> &[TurnEntry] {
        &self.entries
    }
}

/// `~/.ail/projects/<sha1_of_cwd>` — one directory per working directory.
/// Deterministic: same project root always maps to the same bucket, so all
/// runs within a project share a session history directory (SPEC §4.4).
fn project_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut hasher = Sha1::new();
    hasher.update(cwd.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ail")
        .join("projects")
        .join(hash)
}
