//! Per-project pipeline-selection state.
//!
//! Two artifacts live here:
//!
//! 1. **Marker file** (`<cwd>/.ail/default`): a single-line text file naming the
//!    default pipeline for this project. Contents are a path relative to `.ail/`,
//!    e.g. `starter/default.yaml` or `oh-my-ail/.ohmy.ail.yaml`. Hand-editable.
//!
//! 2. **Last-used pointer** (`~/.ail/projects/<sha1_of_cwd>/last_pipeline`): the
//!    absolute path of the most recently successfully-loaded pipeline. Written
//!    by `config::load()` after a clean parse; read by `discovery::discover()`
//!    on subsequent invocations to auto-resume the user's last choice.
//!
//! Both reads are silent — a missing or stale pointer falls through to the next
//! discovery rung. Both writes are best-effort; failures are logged via
//! `tracing::warn!` but never propagated, since persistence is convenience, not
//! correctness.

use std::path::{Path, PathBuf};

use crate::session::log_provider::project_dir;

const MARKER_FILENAME: &str = "default";
const LAST_USED_FILENAME: &str = "last_pipeline";

/// Read the project default-pipeline marker, if present.
///
/// `ail_dir` is the project's `.ail/` directory. The marker file lives at
/// `<ail_dir>/default` and contains a single relative path. The returned
/// `PathBuf` is `<ail_dir>/<marker_contents>`, or `None` if the marker is
/// absent, empty, or points at a file that no longer exists.
pub fn read_marker(ail_dir: &Path) -> Option<PathBuf> {
    let marker = ail_dir.join(MARKER_FILENAME);
    let contents = std::fs::read_to_string(&marker).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return None;
    }
    let resolved = ail_dir.join(trimmed);
    if !resolved.exists() {
        tracing::warn!(
            marker = %marker.display(),
            target = %resolved.display(),
            "ail/default marker points at a missing file — ignoring"
        );
        return None;
    }
    Some(resolved)
}

/// Read the per-project last-used pipeline pointer, if present.
///
/// Returns `None` if the pointer file is missing, empty, or points at a path
/// that no longer exists on disk.
pub fn read_last_used() -> Option<PathBuf> {
    let pointer = project_dir().join(LAST_USED_FILENAME);
    let contents = std::fs::read_to_string(&pointer).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        tracing::debug!(
            pointer = %pointer.display(),
            target = %path.display(),
            "last_pipeline pointer is stale — ignoring"
        );
        return None;
    }
    Some(path)
}

/// Persist the most recently successfully-loaded pipeline path so the next
/// invocation in this project auto-resumes it. Best-effort: errors are logged
/// and swallowed.
pub fn write_last_used(path: &Path) {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let dir = project_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(dir = %dir.display(), error = %e, "failed to create project state dir");
        return;
    }
    let pointer = dir.join(LAST_USED_FILENAME);
    if let Err(e) = std::fs::write(&pointer, abs.to_string_lossy().as_bytes()) {
        tracing::warn!(pointer = %pointer.display(), error = %e, "failed to write last_pipeline pointer");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_marker(tmp.path()).is_none());
    }

    #[test]
    fn marker_returns_none_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(MARKER_FILENAME), "").unwrap();
        assert!(read_marker(tmp.path()).is_none());
    }

    #[test]
    fn marker_returns_none_when_target_missing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(MARKER_FILENAME), "starter/default.yaml").unwrap();
        // Target file doesn't exist — marker silently returns None.
        assert!(read_marker(tmp.path()).is_none());
    }

    #[test]
    fn marker_resolves_relative_path_when_target_exists() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("starter")).unwrap();
        std::fs::write(tmp.path().join("starter/default.yaml"), "x").unwrap();
        std::fs::write(tmp.path().join(MARKER_FILENAME), "starter/default.yaml").unwrap();

        let resolved = read_marker(tmp.path()).unwrap();
        assert_eq!(resolved, tmp.path().join("starter/default.yaml"));
    }

    #[test]
    fn marker_trims_whitespace() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("starter")).unwrap();
        std::fs::write(tmp.path().join("starter/default.yaml"), "x").unwrap();
        std::fs::write(
            tmp.path().join(MARKER_FILENAME),
            "  starter/default.yaml  \n",
        )
        .unwrap();

        let resolved = read_marker(tmp.path()).unwrap();
        assert_eq!(resolved, tmp.path().join("starter/default.yaml"));
    }
}
