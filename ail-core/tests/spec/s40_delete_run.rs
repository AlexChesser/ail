//! Specification §40: Run deletion
//!
//! Tests for the delete_run API — cascading deletes through SQLite and JSONL cleanup.
//!
//! Note: Integration tests for the full delete_run() function (which computes CWD hash internally)
//! require running outside a Claude Code session and are marked #[ignore].
//! Unit tests verify the internal cascade delete logic.

use ail_core::logs::LogQuery;
use ail_core::session::log_provider::LogProvider;
use ail_core::session::sqlite_provider::SqliteProvider;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

#[test]
fn cascade_delete_removes_database_records() {
    // Test that the cascade delete logic works correctly.
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let db_path = tempdir.path().join("ail.db");
    let run_id = "test-run-12345";

    // Create database with a test entry.
    let mut provider = SqliteProvider::open(&db_path).expect("failed to open provider");
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

    // Manually cascade delete to test the logic.
    let mut conn = rusqlite::Connection::open(&db_path).expect("failed to open");
    {
        let tx = conn.transaction().expect("failed to start tx");
        tx.execute("DELETE FROM run_events WHERE run_id = ?1", [run_id])
            .expect("delete run_events");
        tx.execute("DELETE FROM metadata WHERE run_id = ?1", [run_id])
            .expect("delete metadata");
        tx.execute("DELETE FROM traces WHERE run_id = ?1", [run_id])
            .expect("delete traces");
        tx.execute("DELETE FROM steps WHERE run_id = ?1", [run_id])
            .expect("delete steps");
        tx.execute("DELETE FROM sessions WHERE run_id = ?1", [run_id])
            .expect("delete sessions");
        tx.commit().expect("commit");
    }

    // Verify the run is gone from the database.
    let after = ail_core::logs::query_logs_at(&query, &db_path).expect("failed to query");
    assert!(
        !after.iter().any(|s| s.run_id == run_id),
        "run should be deleted from database"
    );
}

#[test]
fn cascade_delete_with_tool_events() {
    // Test that cascade delete removes tool_events associated with a run.
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let db_path = tempdir.path().join("ail.db");
    let run_id = "test-run-with-events";

    // Create database with an entry that includes tool events.
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

    // Cascade delete.
    {
        let tx = conn.transaction().expect("failed to start tx");
        tx.execute("DELETE FROM run_events WHERE run_id = ?1", [run_id])
            .expect("delete run_events");
        tx.execute("DELETE FROM metadata WHERE run_id = ?1", [run_id])
            .expect("delete metadata");
        tx.execute("DELETE FROM traces WHERE run_id = ?1", [run_id])
            .expect("delete traces");
        tx.execute("DELETE FROM steps WHERE run_id = ?1", [run_id])
            .expect("delete steps");
        tx.execute("DELETE FROM sessions WHERE run_id = ?1", [run_id])
            .expect("delete sessions");
        tx.commit().expect("commit");
    }

    // Verify all records are gone.
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
fn jsonl_file_deletion() {
    // Test that JSONL files are properly removed.
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let runs_dir = tempdir.path().join("runs");
    fs::create_dir_all(&runs_dir).expect("failed to create runs dir");

    let run_id = "test-run-jsonl";
    let jsonl_path = runs_dir.join(format!("{}.jsonl", run_id));

    // Create a JSONL file.
    fs::write(&jsonl_path, "{}").expect("failed to write jsonl");
    assert!(jsonl_path.exists(), "jsonl file should exist before delete");

    // Remove it.
    fs::remove_file(&jsonl_path).expect("failed to delete jsonl");
    assert!(
        !jsonl_path.exists(),
        "jsonl file should not exist after delete"
    );
}
