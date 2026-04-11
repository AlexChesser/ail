//! Filesystem utilities — atomic write helper.
//!
//! `atomic_write` writes to a temporary file in the same directory as the target,
//! then renames it into place. On Linux/macOS a `rename(2)` within the same filesystem
//! is atomic and crash-safe: a reader sees either the old file or the new file, never
//! a partial write.

#![allow(clippy::result_large_err)]

use std::io::Write;
use std::path::Path;

use crate::error::AilError;

/// Atomically write `content` to `target`.
///
/// Creates a named temporary file in the same directory as `target`, writes all
/// content, then persists (renames) the temp file to `target`. If the process
/// crashes after the write but before the rename, the temp file is left behind
/// but `target` remains unchanged.
///
/// # Errors
///
/// Returns `AilError::pipeline_aborted` if the temp file cannot be created,
/// written, or renamed. The error detail includes the target path and the
/// underlying OS error.
pub fn atomic_write(target: &Path, content: &[u8]) -> Result<(), AilError> {
    let dir = target.parent().unwrap_or(Path::new("."));

    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| AilError::PipelineAborted {
        detail: format!(
            "Could not create temp file in '{}' for '{}': {e}",
            dir.display(),
            target.display()
        ),
        context: None,
    })?;

    tmp.write_all(content)
        .map_err(|e| AilError::PipelineAborted {
            detail: format!("Could not write temp file for '{}': {e}", target.display()),
            context: None,
        })?;

    tmp.persist(target).map_err(|e| AilError::PipelineAborted {
        detail: format!("Could not rename temp file to '{}': {e}", target.display()),
        context: None,
    })?;

    Ok(())
}

/// Atomically write `content` as UTF-8 to `target`.
///
/// Convenience wrapper around [`atomic_write`] that accepts a string slice.
pub fn atomic_write_str(target: &Path, content: &str) -> Result<(), AilError> {
    atomic_write(target, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn atomic_write_creates_file_with_correct_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.txt");
        atomic_write(&target, b"hello atomic").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello atomic");
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.txt");
        fs::write(&target, "old content").unwrap();
        atomic_write(&target, b"new content").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new content");
    }

    #[test]
    fn atomic_write_str_creates_file_with_correct_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.json");
        atomic_write_str(&target, r#"{"key": "value"}"#).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), r#"{"key": "value"}"#);
    }

    #[test]
    fn atomic_write_nonexistent_dir_returns_error() {
        let target = std::path::PathBuf::from("/nonexistent/dir/output.txt");
        let result = atomic_write(&target, b"content");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::PIPELINE_ABORTED
        );
    }
}
