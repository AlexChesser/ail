use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;
use sha1::{Digest, Sha1};

/// Persistence backend for pipeline run logs.
/// Best-effort: errors are returned to `TurnLog` which logs via `tracing::warn`.
pub trait LogProvider: Send {
    fn write_entry(&mut self, run_id: &str, value: &Value) -> std::io::Result<()>;

    /// Mark the run as finished. Default is a no-op for providers that don't track session state.
    fn finish(&mut self, _run_id: &str, _status: &str) -> std::io::Result<()> {
        Ok(())
    }
}

/// SHA-1 hex digest of the current working directory path.
/// Used to partition project data under `~/.ail/projects/<hash>/` (SPEC §4.4).
pub fn cwd_hash() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut hasher = Sha1::new();
    hasher.update(cwd.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// `~/.ail/projects/<sha1_of_cwd>` — one directory per working directory.
/// Deterministic: same project root always maps to the same bucket, so all
/// runs within a project share a session history directory (SPEC §4.4).
pub fn project_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ail")
        .join("projects")
        .join(cwd_hash())
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

/// Fans out writes to multiple `LogProvider`s. Best-effort: failures in one provider are
/// logged as warnings but do not prevent writes to the remaining providers.
pub struct CompositeProvider {
    providers: Vec<Box<dyn LogProvider>>,
}

impl CompositeProvider {
    pub fn new(providers: Vec<Box<dyn LogProvider>>) -> Self {
        CompositeProvider { providers }
    }
}

impl LogProvider for CompositeProvider {
    fn write_entry(&mut self, run_id: &str, value: &Value) -> std::io::Result<()> {
        for provider in &mut self.providers {
            if let Err(e) = provider.write_entry(run_id, value) {
                tracing::warn!(run_id = %run_id, error = %e, "composite provider: write_entry failed");
            }
        }
        Ok(())
    }

    fn finish(&mut self, run_id: &str, status: &str) -> std::io::Result<()> {
        for provider in &mut self.providers {
            if let Err(e) = provider.finish(run_id, status) {
                tracing::warn!(run_id = %run_id, error = %e, "composite provider: finish failed");
            }
        }
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
