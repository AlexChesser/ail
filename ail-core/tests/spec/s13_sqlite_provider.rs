/// Tests for SqliteProvider and query_logs_at (SPEC §13 — log persistence)

mod sqlite_provider_tests {
    use ail_core::logs::{query_logs_at, LogQuery};
    use ail_core::session::log_provider::LogProvider;
    use ail_core::session::sqlite_provider::SqliteProvider;
    use serde_json::json;

    /// Open a fresh in-memory SQLite provider via a temp file for each test.
    fn open_temp_provider() -> (tempfile::TempDir, SqliteProvider) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ail.db");
        let provider = SqliteProvider::open(&path).unwrap();
        (dir, provider)
    }

    /// SqliteProvider creates the database schema on open.
    #[test]
    fn provider_opens_and_creates_schema() {
        let (_dir, _provider) = open_temp_provider();
        // If we reach here without panic, schema creation succeeded.
    }

    /// A single step entry is stored and queryable.
    #[test]
    fn single_entry_round_trips() {
        let (dir, mut provider) = open_temp_provider();
        let run_id = "test-run-001";
        let entry = json!({
            "step_id": "invocation",
            "type": "step_completed",
            "prompt": "hello world",
            "response": "hi there",
            "cost_usd": 0.001,
            "input_tokens": 10,
            "output_tokens": 5,
        });
        provider.write_entry(run_id, &entry).unwrap();

        let db_path = dir.path().join("ail.db");
        let q = LogQuery {
            session_prefix: None,
            fts_query: None,
            limit: 10,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].run_id, run_id);
        assert_eq!(results[0].steps.len(), 1);
        assert_eq!(results[0].steps[0].step_id, "invocation");
        assert_eq!(results[0].steps[0].response.as_deref(), Some("hi there"));
    }

    /// Multiple sessions are returned, ordered by started_at DESC.
    #[test]
    fn multiple_sessions_ordered_descending() {
        let (dir, mut provider) = open_temp_provider();

        provider
            .write_entry("run-a", &json!({"step_id": "s1", "type": "step_completed"}))
            .unwrap();
        // Small sleep to ensure distinct timestamps (millisecond resolution).
        std::thread::sleep(std::time::Duration::from_millis(2));
        provider
            .write_entry("run-b", &json!({"step_id": "s1", "type": "step_completed"}))
            .unwrap();

        let db_path = dir.path().join("ail.db");
        let q = LogQuery {
            session_prefix: None,
            fts_query: None,
            limit: 10,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert_eq!(results.len(), 2);
        // Most recent session first.
        assert_eq!(results[0].run_id, "run-b");
        assert_eq!(results[1].run_id, "run-a");
    }

    /// Session prefix filter restricts results.
    #[test]
    fn session_prefix_filter() {
        let (dir, mut provider) = open_temp_provider();
        provider
            .write_entry(
                "abc-123",
                &json!({"step_id": "s", "type": "step_completed"}),
            )
            .unwrap();
        provider
            .write_entry(
                "def-456",
                &json!({"step_id": "s", "type": "step_completed"}),
            )
            .unwrap();

        let db_path = dir.path().join("ail.db");
        let q = LogQuery {
            session_prefix: Some("abc".to_string()),
            fts_query: None,
            limit: 10,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].run_id, "abc-123");
    }

    /// FTS query returns matching sessions only.
    #[test]
    fn fts_query_filters_by_content() {
        let (dir, mut provider) = open_temp_provider();
        provider
            .write_entry(
                "run-fizzbuzz",
                &json!({
                    "step_id": "invocation",
                    "type": "step_completed",
                    "prompt": "write a fizzbuzz function",
                    "response": "here is fizzbuzz",
                }),
            )
            .unwrap();
        provider
            .write_entry(
                "run-hello",
                &json!({
                    "step_id": "invocation",
                    "type": "step_completed",
                    "prompt": "say hello",
                    "response": "hello world",
                }),
            )
            .unwrap();

        let db_path = dir.path().join("ail.db");
        let q = LogQuery {
            session_prefix: None,
            fts_query: Some("fizzbuzz".to_string()),
            limit: 10,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].run_id, "run-fizzbuzz");
    }

    /// Returns an empty Vec (not an error) when the database does not exist.
    #[test]
    fn returns_empty_when_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");
        let q = LogQuery {
            session_prefix: None,
            fts_query: None,
            limit: 10,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert!(results.is_empty());
    }

    /// Limit parameter caps the number of returned sessions.
    #[test]
    fn limit_caps_results() {
        let (dir, mut provider) = open_temp_provider();
        for i in 0..10u32 {
            std::thread::sleep(std::time::Duration::from_millis(1));
            provider
                .write_entry(
                    &format!("run-{i}"),
                    &json!({"step_id": "s", "type": "step_completed"}),
                )
                .unwrap();
        }

        let db_path = dir.path().join("ail.db");
        let q = LogQuery {
            session_prefix: None,
            fts_query: None,
            limit: 3,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert_eq!(results.len(), 3);
    }

    /// cost_usd is accumulated on the session row from step entries.
    #[test]
    fn session_cost_accumulates() {
        let (dir, mut provider) = open_temp_provider();
        provider
            .write_entry(
                "run-cost",
                &json!({"step_id": "s1", "type": "step_completed", "cost_usd": 0.10}),
            )
            .unwrap();
        provider
            .write_entry(
                "run-cost",
                &json!({"step_id": "s2", "type": "step_completed", "cost_usd": 0.05}),
            )
            .unwrap();

        let db_path = dir.path().join("ail.db");
        let q = LogQuery {
            session_prefix: Some("run-cost".to_string()),
            fts_query: None,
            limit: 10,
        };
        let results = query_logs_at(&q, &db_path).unwrap();
        assert_eq!(results.len(), 1);
        let total = results[0].total_cost_usd.unwrap_or(0.0);
        assert!((total - 0.15).abs() < 1e-9, "expected 0.15, got {total}");
    }
}
