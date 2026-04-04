//! Delete execution logs from the database and JSONL files.
//!
//! Provides the ability to delete individual runs or bulk runs,
//! cascading deletes through SQLite tables and removing associated JSONL files.

#![allow(clippy::result_large_err)]

use std::path::PathBuf;

use rusqlite::Connection;
use sha1::{Digest, Sha1};

use crate::error::{error_types, AilError};

/// Delete a single run from the database and remove its JSONL file.
///
/// This is the primary public API. It computes the CWD hash internally.
///
/// Performs a cascading delete respecting the schema structure:
/// - `run_events` (run_id references)
/// - `metadata` (run_id references)
/// - `traces_fts` (FTS virtual table, via trigger on traces)
/// - `traces` (run_id references)
/// - `steps` (run_id references)
/// - `sessions` (run_id primary key)
/// - JSONL file at `~/.ail/projects/<project_hash>/runs/<run_id>.jsonl`
///
/// If `force` is false, returns an error if the JSONL file does not exist (protecting against
/// accidental deletion of records without corresponding log files). If `force` is true, deletes
/// the database records even if the JSONL file is missing.
pub fn delete_run(run_id: &str, force: bool) -> Result<(), AilError> {
    let cwd = std::env::current_dir().map_err(|e| AilError {
        error_type: "ail:io/cwd-access",
        title: "Failed to determine current working directory",
        detail: e.to_string(),
        context: None,
    })?;

    let mut hasher = Sha1::new();
    hasher.update(cwd.to_string_lossy().as_bytes());
    let cwd_hash = format!("{:x}", hasher.finalize());

    delete_run_at(run_id, &cwd_hash, force)
}

/// Delete a single run with explicit CWD hash. Used internally and for testing.
///
/// If `force` is false, returns an error if the JSONL file does not exist (protecting against
/// accidental deletion of records without corresponding log files). If `force` is true, deletes
/// the database records even if the JSONL file is missing.
fn delete_run_at(
    run_id: &str,
    cwd_hash: &str,
    force: bool,
) -> Result<(), AilError> {
    let db_path = project_dir_for_hash(cwd_hash).join("ail.db");

    // Check JSONL file existence unless force is set.
    let jsonl_path = project_dir_for_hash(cwd_hash)
        .join("runs")
        .join(format!("{run_id}.jsonl"));

    if !force && !jsonl_path.exists() {
        return Err(AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Run not found",
            detail: format!(
                "No JSONL file found for run {}. Use --force to delete database records only.",
                run_id
            ),
            context: None,
        });
    }

    // Open database connection.
    let mut conn = Connection::open(&db_path).map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to open database",
        detail: e.to_string(),
        context: None,
    })?;

    // Verify the run exists in the database.
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
            [run_id],
            |row| row.get::<_, u32>(0).map(|c| c > 0),
        )
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to check run existence",
            detail: e.to_string(),
            context: None,
        })?;

    if !exists {
        return Err(AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Run not found",
            detail: format!("No session found with run_id {}", run_id),
            context: None,
        });
    }

    // Start transaction for atomic cascade delete.
    let tx = conn.transaction().map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to start transaction",
        detail: e.to_string(),
        context: None,
    })?;

    // Delete in cascade order (respecting any foreign key relationships).
    // Note: no explicit foreign key constraints in the schema, but we follow
    // the logical dependency order.

    tx.execute("DELETE FROM run_events WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to delete run_events",
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM metadata WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to delete metadata",
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM traces WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to delete traces",
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM steps WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to delete steps",
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM sessions WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to delete session",
            detail: e.to_string(),
            context: None,
        })?;

    tx.commit().map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to commit transaction",
        detail: e.to_string(),
        context: None,
    })?;

    // Delete JSONL file if it exists.
    if jsonl_path.exists() {
        std::fs::remove_file(&jsonl_path).map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to delete JSONL file",
            detail: format!(
                "Could not delete {}: {}",
                jsonl_path.display(),
                e
            ),
            context: None,
        })?;
    }

    Ok(())
}

/// Delete multiple runs from the database and remove their JSONL files.
///
/// Deletes the specified runs one by one. If `force` is false, stops at the first
/// run that doesn't have a corresponding JSONL file. If `force` is true, continues
/// deleting even if some JSONL files are missing.
///
/// Returns the count of successfully deleted runs.
pub fn delete_runs(
    run_ids: &[String],
    force: bool,
) -> Result<usize, AilError> {
    let mut deleted_count = 0;

    for run_id in run_ids {
        match delete_run(run_id, force) {
            Ok(()) => {
                deleted_count += 1;
            }
            Err(e) => {
                if !force {
                    return Err(e);
                }
                // Log warning and continue if force is set.
                tracing::warn!(run_id = %run_id, error = %e, "failed to delete run, continuing");
            }
        }
    }

    Ok(deleted_count)
}

/// Helper: compute project directory for a given CWD hash.
/// Used by both delete_run and tests.
fn project_dir_for_hash(cwd_hash: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ail")
        .join("projects")
        .join(cwd_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_dir_for_hash_returns_correct_path() {
        let hash = "abcd1234";
        let path = project_dir_for_hash(hash);
        assert!(path.ends_with("abcd1234"));
        assert!(path.to_string_lossy().contains(".ail/projects"));
    }
}
