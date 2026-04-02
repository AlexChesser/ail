use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;
use sha1::{Digest, Sha1};

/// Persistence backend for pipeline run logs.
/// Best-effort: errors are returned to `TurnLog` which logs via `tracing::warn`.
pub trait LogProvider: Send {
    fn write_entry(&mut self, run_id: &str, value: &Value) -> std::io::Result<()>;
}

/// `~/.ail/projects/<sha1_of_cwd>` — one directory per working directory.
/// Deterministic: same project root always maps to the same bucket, so all
/// runs within a project share a session history directory (SPEC §4.4).
pub fn project_dir() -> PathBuf {
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

/// Full path to the NDJSON run log file for a given run_id.
pub fn run_path(run_id: &str) -> PathBuf {
    project_dir().join("runs").join(format!("{run_id}.jsonl"))
}

/// Default NDJSON file provider. Writes to `~/.ail/projects/<sha1>/runs/<run_id>.jsonl`.
pub struct JsonlProvider {
    project_dir: PathBuf,
}

impl JsonlProvider {
    pub fn new() -> Self {
        JsonlProvider {
            project_dir: project_dir(),
        }
    }

    pub fn run_path(&self, run_id: &str) -> PathBuf {
        self.project_dir
            .join("runs")
            .join(format!("{run_id}.jsonl"))
    }
}

impl Default for JsonlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LogProvider for JsonlProvider {
    fn write_entry(&mut self, run_id: &str, value: &Value) -> std::io::Result<()> {
        let dir = self.project_dir.join("runs");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{run_id}.jsonl"));
        let line = serde_json::to_string(value).map_err(std::io::Error::other)?;
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{line}")
    }
}

/// No-op provider for tests that don't need file I/O.
pub struct NullProvider;

impl LogProvider for NullProvider {
    fn write_entry(&mut self, _run_id: &str, _value: &Value) -> std::io::Result<()> {
        Ok(())
    }
}

/// Test support types. Not intended for production use.
pub mod test_support {
    use super::*;

    /// Captures all written entries into a `Vec<Value>` for assertion in tests.
    pub struct CapturingProvider {
        pub entries: Vec<Value>,
    }

    impl Default for CapturingProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CapturingProvider {
        pub fn new() -> Self {
            CapturingProvider {
                entries: Vec::new(),
            }
        }
    }

    impl LogProvider for CapturingProvider {
        fn write_entry(&mut self, _run_id: &str, value: &Value) -> std::io::Result<()> {
            self.entries.push(value.clone());
            Ok(())
        }
    }
}
