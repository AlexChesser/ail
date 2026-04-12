//! Delete execution logs from the database and JSONL files.
//!
//! Provides the ability to delete individual runs or bulk runs,
//! cascading deletes through SQLite tables and removing associated JSONL files.

#![allow(clippy::result_large_err)]

use std::path::PathBuf;

use rusqlite::Connection;

use crate::error::AilError;
use crate::session::cwd_hash as compute_cwd_hash;

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
    delete_run_at(run_id, &compute_cwd_hash(), force)
}

/// Delete a single run with explicit CWD hash. Used internally and for testing.
///
/// If `force` is false, returns an error if the JSONL file does not exist (protecting against
/// accidental deletion of records without corresponding log files). If `force` is true, deletes
/// the database records even if the JSONL file is missing.
pub(crate) fn delete_run_at(run_id: &str, cwd_hash: &str, force: bool) -> Result<(), AilError> {
    let db_path = project_dir_for_hash(cwd_hash).join("ail.db");

    // Check JSONL file existence unless force is set.
    let jsonl_path = project_dir_for_hash(cwd_hash)
        .join("runs")
        .join(format!("{run_id}.jsonl"));

    if !force && !jsonl_path.exists() {
        return Err(AilError::RunNotFound {
            detail: format!(
                "No JSONL file found for run {}. Use --force to delete database records only.",
                run_id
            ),
            context: None,
        });
    }

    let mut conn = Connection::open(&db_path).map_err(|e| AilError::StorageDeleteFailed {
        detail: e.to_string(),
        context: None,
    })?;

    delete_run_from_conn(&mut conn, run_id)?;

    // Delete JSONL file if it exists.
    if jsonl_path.exists() {
        std::fs::remove_file(&jsonl_path).map_err(|e| AilError::StorageDeleteFailed {
            detail: format!("Could not delete {}: {}", jsonl_path.display(), e),
            context: None,
        })?;
    }

    Ok(())
}

/// Cascade-delete a single run from an already-open database connection.
///
/// Verifies the run exists, then atomically removes all associated rows from:
/// `run_events`, `metadata`, `traces`, `steps`, and `sessions`.
///
/// Exposed as `pub` so integration tests can call through the real production
/// logic without needing filesystem fixtures for the database path.
pub fn delete_run_from_conn(conn: &mut Connection, run_id: &str) -> Result<(), AilError> {
    // Verify the run exists in the database.
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
            [run_id],
            |row| row.get::<_, u32>(0).map(|c| c > 0),
        )
        .map_err(|e| AilError::StorageQueryFailed {
            detail: e.to_string(),
            context: None,
        })?;

    if !exists {
        return Err(AilError::RunNotFound {
            detail: format!("No session found with run_id {}", run_id),
            context: None,
        });
    }

    // Start transaction for atomic cascade delete.
    // Note: no explicit foreign key constraints in the schema, but we follow
    // the logical dependency order.
    let tx = conn
        .transaction()
        .map_err(|e| AilError::StorageDeleteFailed {
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM run_events WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError::StorageDeleteFailed {
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM metadata WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError::StorageDeleteFailed {
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM traces WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError::StorageDeleteFailed {
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM steps WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError::StorageDeleteFailed {
            detail: e.to_string(),
            context: None,
        })?;

    tx.execute("DELETE FROM sessions WHERE run_id = ?1", [run_id])
        .map_err(|e| AilError::StorageDeleteFailed {
            detail: e.to_string(),
            context: None,
        })?;

    tx.commit().map_err(|e| AilError::StorageDeleteFailed {
        detail: e.to_string(),
        context: None,
    })?;

    Ok(())
}

/// Delete multiple runs from the database and remove their JSONL files.
///
/// Deletes the specified runs one by one. If `force` is false, stops at the first
/// run that doesn't have a corresponding JSONL file. If `force` is true, continues
/// deleting even if some JSONL files are missing.
///
/// Returns the count of successfully deleted runs.
pub fn delete_runs(run_ids: &[String], force: bool) -> Result<usize, AilError> {
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
pub(crate) fn project_dir_for_hash(cwd_hash: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ail")
        .join("projects")
        .join(cwd_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn project_dir_for_hash_returns_correct_path() {
        let hash = "abcd1234";
        let path = project_dir_for_hash(hash);
        assert!(path.ends_with("abcd1234"));
        assert!(path.to_string_lossy().contains(".ail/projects"));
    }

    #[test]
    fn project_dir_for_hash_contains_hash_segment() {
        let hash = "deadbeef99";
        let path = project_dir_for_hash(hash);
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("deadbeef99"),
            "path should contain the hash segment"
        );
    }

    #[test]
    fn project_dir_for_hash_different_hashes_produce_different_paths() {
        let path_a = project_dir_for_hash("hash_a");
        let path_b = project_dir_for_hash("hash_b");
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn delete_run_at_missing_jsonl_without_force_returns_error() {
        let tempdir = TempDir::new().expect("tempdir");
        let run_id = "no-jsonl-run";

        // delete_run_at resolves paths via dirs::home_dir(). We pass a synthetic hash
        // that does not exist under home, so the JSONL file will not be found.
        let fake_hash = "hash-no-jsonl";
        let _ = tempdir; // kept to make intent clear

        // Call with a synthetic hash; the path won't exist so force=false should error.
        let result = delete_run_at(run_id, fake_hash, false);
        assert!(
            result.is_err(),
            "should return error when JSONL file is missing and force=false"
        );
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), crate::error::error_types::RUN_NOT_FOUND);
        assert!(
            err.detail().contains(run_id),
            "error detail should mention the run_id"
        );
    }

    /// Full end-to-end: delete_run_at removes DB records and the JSONL file.
    ///
    /// Overrides HOME to a tempdir so project_dir_for_hash resolves inside the tempdir.
    /// Note: HOME override is not thread-safe; this test must not run concurrently with
    /// tests that also mutate HOME (none currently do, and nextest isolates processes).
    #[test]
    fn delete_run_at_happy_path_removes_db_records_and_jsonl() {
        use crate::session::log_provider::LogProvider;
        use crate::session::sqlite_provider::SqliteProvider;
        use serde_json::json;
        use std::fs;

        let home_dir = TempDir::new().expect("tempdir");
        // Override HOME so that project_dir_for_hash resolves inside our tempdir.
        let orig_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_dir.path());

        let hash = "test-happy-delete-hash";
        let run_id = "happy-delete-run";

        // Create directory structure: $HOME/.ail/projects/<hash>/runs/
        let project_dir = home_dir.path().join(".ail").join("projects").join(hash);
        let runs_dir = project_dir.join("runs");
        fs::create_dir_all(&runs_dir).expect("create runs dir");

        // Write DB records.
        let db_path = project_dir.join("ail.db");
        let mut provider = SqliteProvider::open(&db_path).expect("open sqlite");
        provider
            .write_entry(
                run_id,
                &json!({
                    "step_id": "invocation",
                    "type": "step_completed",
                    "prompt": "hello",
                    "response": "world",
                }),
            )
            .expect("write entry");
        drop(provider);

        // Write JSONL file.
        let jsonl_path = runs_dir.join(format!("{run_id}.jsonl"));
        fs::write(&jsonl_path, r#"{"type":"step_completed"}"#).expect("write jsonl");
        assert!(jsonl_path.exists(), "jsonl must exist before delete");

        // Run the real production code.
        let result = delete_run_at(run_id, hash, false);

        // Restore HOME before any assertions so other tests aren't affected.
        match orig_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }

        result.expect("delete_run_at should succeed");
        assert!(
            !jsonl_path.exists(),
            "jsonl file should be gone after delete"
        );

        // Verify DB record is removed.
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
                [run_id],
                |row| row.get(0),
            )
            .expect("count query");
        assert_eq!(count, 0, "session row should be deleted from DB");
    }

    #[test]
    fn delete_run_at_missing_jsonl_with_force_proceeds_to_db_open() {
        // With force=true and a non-existent DB, we get a DB-related error (not a JSONL error).
        let run_id = "force-no-jsonl-run";
        let fake_hash = "hash-force-no-db";

        let result = delete_run_at(run_id, fake_hash, true);
        // Should not get a RUN_NOT_FOUND (JSONL) error. Instead we get a StorageDeleteFailed
        // (since the DB doesn't exist either). Either way the JSONL check is bypassed.
        if let Err(e) = result {
            assert!(
                !e.detail().contains("No JSONL file found"),
                "with force=true the JSONL check should be bypassed, got: {}",
                e.detail()
            );
        }
        // If somehow it succeeds (empty DB), that's also fine.
    }
}
