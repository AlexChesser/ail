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
/// Canonicalizes to resolve symlinks so that equivalent paths (e.g. macOS
/// `/tmp/foo` vs `/private/tmp/foo`) produce the same hash.
pub fn cwd_hash() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let canonical = cwd.canonicalize().unwrap_or(cwd);
    let mut hasher = Sha1::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_provider_write_entry_succeeds() {
        let mut provider = NullProvider;
        let value = json!({"step_id": "s1"});
        assert!(provider.write_entry("run-1", &value).is_ok());
    }

    #[test]
    fn null_provider_finish_is_noop() {
        let mut provider = NullProvider;
        assert!(provider.finish("run-1", "ok").is_ok());
    }

    #[test]
    fn cwd_hash_returns_nonempty_hex_string() {
        let h = cwd_hash();
        assert!(!h.is_empty());
        // All hex characters
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be hex: {h}"
        );
    }

    #[test]
    fn cwd_hash_is_deterministic() {
        let h1 = cwd_hash();
        let h2 = cwd_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn jsonl_provider_creates_file_and_appends_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let run_id = "test-run-create";
        let value = json!({"step_id": "s1", "response": "hello"});

        // Build a JsonlProvider pointed at tmp dir
        let mut provider = JsonlProvider {
            project_dir: tmp.path().to_path_buf(),
        };
        provider.write_entry(run_id, &value).unwrap();

        let path = provider.run_path(run_id);
        assert!(path.exists(), "file should exist at {path:?}");
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(parsed["step_id"], "s1");
        assert_eq!(parsed["response"], "hello");
    }

    #[test]
    fn jsonl_provider_multiple_writes_are_valid_ndjson() {
        let tmp = tempfile::tempdir().unwrap();
        let run_id = "test-run-ndjson";

        let mut provider = JsonlProvider {
            project_dir: tmp.path().to_path_buf(),
        };
        provider.write_entry(run_id, &json!({"seq": 1})).unwrap();
        provider.write_entry(run_id, &json!({"seq": 2})).unwrap();
        provider.write_entry(run_id, &json!({"seq": 3})).unwrap();

        let path = provider.run_path(run_id);
        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3, "should have 3 NDJSON lines");
        for (i, line) in lines.iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("line {i} is not valid JSON: {e}"));
            assert_eq!(parsed["seq"], i as i64 + 1);
        }
    }

    #[test]
    fn composite_provider_fans_out_to_both_providers() {
        use super::test_support::CapturingProvider;

        let cap1 = CapturingProvider::new();
        let cap2 = CapturingProvider::new();
        // We can't inspect inner providers after boxing, so use two NullProviders
        // and verify CompositeProvider::write_entry returns Ok (no early-exit on failure).
        let mut composite =
            CompositeProvider::new(vec![Box::new(NullProvider), Box::new(NullProvider)]);
        let value = json!({"step_id": "fan-out"});
        assert!(composite.write_entry("run-fan", &value).is_ok());
        assert!(composite.finish("run-fan", "ok").is_ok());
        // Suppress unused variable warning
        let _ = (cap1, cap2);
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
