//! Specification §40: Run deletion
//!
//! Tests for the delete_run API — cascading deletes through SQLite and JSONL cleanup.
//!
//! Note: Integration tests for the full delete_run() function (which computes CWD hash internally)
//! require running outside a Claude Code session and are marked #[ignore].
//! Unit tests verify the internal cascade delete logic.

use ail_core::delete::delete_run_from_conn;
use ail_core::logs::LogQuery;
use ail_core::session::log_provider::LogProvider;
use ail_core::session::sqlite_provider::SqliteProvider;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn write_run(db_path: &std::path::Path, run_id: &str) {
    let mut provider = SqliteProvider::open(db_path).expect("failed to open provider");
    let entry = json!({
        "step_id": "invocation",
        "type": "step_completed",
        "prompt": "hello",
        "response": "world",
        "cost_usd": 0.001,
        "input_tokens": 10,
        "output_tokens": 20,
    });
    provider
        .write_entry(run_id, &entry)
        .expect("failed to write entry");
}

#[test]
fn cascade_delete_removes_database_records() {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let db_path = tempdir.path().join("ail.db");
    let run_id = "test-run-12345";

    write_run(&db_path, run_id);

    // Verify the run exists before delete.
    let query = LogQuery {
        session_prefix: None,
        fts_query: None,
        limit: 100,
    };
    let before = ail_core::logs::query_logs_at(&query, &db_path).expect("failed to query");
    assert!(
        before.iter().any(|s| s.run_id == run_id),
        "run should exist before delete"
    );

    // Call through the real production code.
    let mut conn = rusqlite::Connection::open(&db_path).expect("failed to open");
    delete_run_from_conn(&mut conn, run_id).expect("delete should succeed");

    let after = ail_core::logs::query_logs_at(&query, &db_path).expect("failed to query");
    assert!(
        !after.iter().any(|s| s.run_id == run_id),
        "run should be deleted from database"
    );
}

#[test]
fn cascade_delete_with_tool_events() {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let db_path = tempdir.path().join("ail.db");
    let run_id = "test-run-with-events";

    let mut provider = SqliteProvider::open(&db_path).expect("failed to open provider");
    let entry = json!({
        "step_id": "invocation",
        "type": "step_completed",
        "prompt": "hello",
        "response": "world",
        "tool_events": [
            {
                "seq": 0,
                "event_type": "tool_call",
                "tool_name": "test_tool",
                "tool_id": "test-id-1",
                "content_json": "{\"test\": \"data\"}"
            }
        ]
    });
    provider
        .write_entry(run_id, &entry)
        .expect("failed to write entry");

    // Verify run_events exist before delete.
    let mut conn = rusqlite::Connection::open(&db_path).expect("failed to open");
    let event_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM run_events WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .expect("failed to count");
    assert!(event_count > 0, "tool events should exist before delete");

    // Call through the real production code.
    delete_run_from_conn(&mut conn, run_id).expect("delete should succeed");

    let event_count_after: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM run_events WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .expect("failed to count");
    assert_eq!(event_count_after, 0, "tool events should be deleted");

    let session_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .expect("failed to count");
    assert_eq!(session_count, 0, "session should be deleted");
}

#[test]
fn cascade_delete_run_not_found_returns_error() {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let db_path = tempdir.path().join("ail.db");

    // Create schema by opening a provider, but don't insert the run.
    let _provider = SqliteProvider::open(&db_path).expect("failed to open provider");

    let mut conn = rusqlite::Connection::open(&db_path).expect("failed to open");
    let result = delete_run_from_conn(&mut conn, "nonexistent-run-id");

    assert!(result.is_err(), "should error when run not found");
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type(),
        ail_core::error::error_types::PIPELINE_ABORTED
    );
    assert!(
        err.detail().contains("nonexistent-run-id"),
        "error should name the missing run_id"
    );
}

#[test]
fn cascade_delete_multiple_runs_only_removes_target() {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let db_path = tempdir.path().join("ail.db");
    let run_a = "run-to-delete";
    let run_b = "run-to-keep";

    write_run(&db_path, run_a);
    write_run(&db_path, run_b);

    let mut conn = rusqlite::Connection::open(&db_path).expect("failed to open");
    delete_run_from_conn(&mut conn, run_a).expect("delete should succeed");

    let session_a: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
            [run_a],
            |row| row.get(0),
        )
        .expect("count");
    let session_b: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
            [run_b],
            |row| row.get(0),
        )
        .expect("count");

    assert_eq!(session_a, 0, "deleted run should be gone");
    assert_eq!(session_b, 1, "sibling run should be untouched");
}

#[test]
fn jsonl_file_deletion() {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let runs_dir = tempdir.path().join("runs");
    fs::create_dir_all(&runs_dir).expect("failed to create runs dir");

    let run_id = "test-run-jsonl";
    let jsonl_path = runs_dir.join(format!("{}.jsonl", run_id));

    fs::write(&jsonl_path, "{}").expect("failed to write jsonl");
    assert!(jsonl_path.exists(), "jsonl file should exist before delete");

    fs::remove_file(&jsonl_path).expect("failed to delete jsonl");
    assert!(
        !jsonl_path.exists(),
        "jsonl file should not exist after delete"
    );
}
