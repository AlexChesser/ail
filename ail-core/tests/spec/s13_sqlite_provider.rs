use ail_core::session::log_provider::LogProvider;
use ail_core::session::sqlite_provider::SqliteProvider;
use serde_json::json;
use tempfile::tempdir;

fn open_db(dir: &std::path::Path) -> SqliteProvider {
    SqliteProvider::open(dir.join("ail.db")).expect("SqliteProvider::open failed")
}

#[test]
fn sqlite_provider_creates_db_and_tables() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("ail.db");

    // DB should not exist yet.
    assert!(!db_path.exists());

    let _provider = open_db(dir.path());

    // After construction the file must exist.
    assert!(db_path.exists(), "ail.db was not created");

    // Verify the four expected tables exist.
    let conn = rusqlite::Connection::open(&db_path).expect("open db");
    for table in &["sessions", "steps", "metadata"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .expect("query sqlite_master");
        assert_eq!(count, 1, "table '{table}' missing");
    }
    // FTS5 virtual tables appear as type 'table' in sqlite_master.
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='traces'",
            [],
            |r| r.get(0),
        )
        .expect("query traces");
    assert_eq!(count, 1, "virtual table 'traces' missing");
}

#[test]
fn sqlite_provider_records_step_started() {
    let dir = tempdir().expect("tempdir");
    let mut provider = open_db(dir.path());

    let event = json!({
        "type": "step_started",
        "step_id": "review",
        "prompt": "Please review the diff."
    });
    provider
        .write_entry("run-002", &event)
        .expect("write_entry");

    let conn = rusqlite::Connection::open(dir.path().join("ail.db")).expect("open db");

    // Session row created with status='running'.
    let status: String = conn
        .query_row(
            "SELECT status FROM sessions WHERE run_id = 'run-002'",
            [],
            |r| r.get(0),
        )
        .expect("session row");
    assert_eq!(status, "running");

    // Steps row inserted with correct event_type and step_id.
    let (event_type, step_id, prompt): (String, String, String) = conn
        .query_row(
            "SELECT event_type, step_id, prompt FROM steps WHERE run_id = 'run-002'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("steps row");
    assert_eq!(event_type, "step_started");
    assert_eq!(step_id, "review");
    assert_eq!(prompt, "Please review the diff.");
}

#[test]
fn sqlite_provider_records_turn_entry() {
    let dir = tempdir().expect("tempdir");
    let mut provider = open_db(dir.path());

    // TurnEntry-shaped value: has step_id but no "type" field.
    let entry = json!({
        "step_id": "summarise",
        "prompt": "Summarise this.",
        "response": "Here is a summary.",
        "cost_usd": 0.001,
        "input_tokens": 100,
        "output_tokens": 50,
        "thinking": null,
        "stdout": null,
        "stderr": null,
        "exit_code": null,
        "runner_session_id": null
    });
    provider
        .write_entry("run-003", &entry)
        .expect("write_entry");

    let conn = rusqlite::Connection::open(dir.path().join("ail.db")).expect("open db");

    // Steps row inserted with event_type='turn_entry'.
    let (event_type, response, cost): (String, Option<String>, Option<f64>) = conn
        .query_row(
            "SELECT event_type, response, cost_usd FROM steps WHERE run_id = 'run-003'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("steps row");
    assert_eq!(event_type, "turn_entry");
    assert_eq!(response.as_deref(), Some("Here is a summary."));
    assert!((cost.unwrap_or(0.0) - 0.001).abs() < 1e-9);

    // Cost accumulated in session.
    let total: f64 = conn
        .query_row(
            "SELECT total_cost_usd FROM sessions WHERE run_id = 'run-003'",
            [],
            |r| r.get(0),
        )
        .expect("session total_cost_usd");
    assert!((total - 0.001).abs() < 1e-9);

    // FTS row created for the response content.
    let fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM traces WHERE run_id = 'run-003'",
            [],
            |r| r.get(0),
        )
        .expect("fts count");
    assert_eq!(fts_count, 1);
}

#[test]
fn sqlite_provider_fts_search() {
    let dir = tempdir().expect("tempdir");
    let mut provider = open_db(dir.path());

    let entry1 = json!({
        "step_id": "step-a",
        "prompt": "Do something.",
        "response": "The refactoring is now complete and tests pass.",
        "cost_usd": null,
        "input_tokens": 0,
        "output_tokens": 0,
        "runner_session_id": null
    });
    let entry2 = json!({
        "step_id": "step-b",
        "prompt": "Do something else.",
        "response": "Added documentation for all public functions.",
        "cost_usd": null,
        "input_tokens": 0,
        "output_tokens": 0,
        "runner_session_id": null
    });
    provider
        .write_entry("run-004", &entry1)
        .expect("write_entry 1");
    provider
        .write_entry("run-004", &entry2)
        .expect("write_entry 2");

    let conn = rusqlite::Connection::open(dir.path().join("ail.db")).expect("open db");

    // FTS query for "refactor" (porter stemmer should match "refactoring").
    let matches: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM traces WHERE content MATCH 'refactor'",
            [],
            |r| r.get(0),
        )
        .expect("fts match refactor");
    assert_eq!(matches, 1, "expected one FTS hit for 'refactor'");

    // FTS query for "document" (should match "documentation").
    let matches2: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM traces WHERE content MATCH 'document'",
            [],
            |r| r.get(0),
        )
        .expect("fts match document");
    assert_eq!(matches2, 1, "expected one FTS hit for 'document'");

    // A term that matches nothing.
    let no_matches: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM traces WHERE content MATCH 'xyzzy'",
            [],
            |r| r.get(0),
        )
        .expect("fts no match");
    assert_eq!(no_matches, 0, "expected no FTS hits for 'xyzzy'");
}
