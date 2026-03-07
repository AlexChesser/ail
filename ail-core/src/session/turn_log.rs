use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;

use serde::Serialize;

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

pub struct TurnLog {
    entries: Vec<TurnEntry>,
    run_id: String,
}

impl TurnLog {
    pub fn new(run_id: String) -> Self {
        TurnLog {
            entries: Vec::new(),
            run_id,
        }
    }

    pub fn append(&mut self, entry: TurnEntry) {
        tracing::info!(
            run_id = %self.run_id,
            step_id = %entry.step_id,
            cost_usd = ?entry.cost_usd,
            "turn_log_append"
        );

        // Persist to .ail/runs/<run_id>.jsonl (append-only)
        if let Err(e) = self.write_ndjson_line(&entry) {
            tracing::warn!(run_id = %self.run_id, error = %e, "failed to persist turn log entry");
        }

        self.entries.push(entry);
    }

    fn write_ndjson_line(&self, entry: &TurnEntry) -> std::io::Result<()> {
        let dir = std::path::Path::new(".ail/runs");
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.jsonl", self.run_id));
        let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
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
