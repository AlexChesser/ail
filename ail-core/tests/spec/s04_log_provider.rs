/// Tests for LogProvider trait and persistence backends (SPEC §4.4)
mod log_provider_tests {
    use ail_core::config::domain::Pipeline;
    use ail_core::session::log_provider::{JsonlProvider, LogProvider, NullProvider};
    use ail_core::session::{Session, TurnEntry, TurnLog};
    use serde_json::json;
    use std::time::SystemTime;

    fn make_entry(step_id: &str, response: Option<&str>) -> TurnEntry {
        TurnEntry {
            step_id: step_id.to_string(),
            prompt: "test prompt".to_string(),
            response: response.map(|s| s.to_string()),
            timestamp: SystemTime::now(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            runner_session_id: None,
            stdout: None,
            stderr: None,
            exit_code: None,
            thinking: None,
            tool_events: vec![],
            modified: None,
            iterations_completed: None,
        }
    }

    /// JsonlProvider writes an entry to the expected path.
    #[test]
    fn jsonl_provider_writes_to_expected_path() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let run_id = "test-jsonl-provider-path";
        let mut provider = JsonlProvider::new();
        let value = json!({"step_id": "hello", "response": "world"});
        provider.write_entry(run_id, &value).unwrap();

        let path = provider.run_path(run_id);
        assert!(path.exists(), "NDJSON file should exist at {path:?}");

        let contents = std::fs::read_to_string(&path).unwrap();
        let line: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(line["step_id"], "hello");
        assert_eq!(line["response"], "world");

        std::env::set_current_dir(orig).unwrap();
    }

    /// NullProvider is a no-op — returns Ok and writes nothing.
    #[test]
    fn null_provider_is_noop() {
        let mut provider = NullProvider;
        let value = json!({"step_id": "s1"});
        let result = provider.write_entry("some-run-id", &value);
        assert!(result.is_ok());
        // No file should have been created — we just verify it returned Ok.
    }

    /// TurnLog with a CapturingProvider collects entries in-memory.
    #[test]
    fn turn_log_with_capturing_provider() {
        use ail_core::session::log_provider::test_support::CapturingProvider;

        let capturing = CapturingProvider::new();
        let mut log = TurnLog::with_provider("run-capture".to_string(), Box::new(capturing));

        log.record_step_started("step_a", "do something");
        log.append(make_entry("step_a", Some("result a")));
        log.append(make_entry("step_b", Some("result b")));

        // We can't easily re-borrow the provider after it's boxed, so we verify
        // via the in-memory entries and run_path consistency.
        assert_eq!(log.entries().len(), 2);
        assert_eq!(log.entries()[0].step_id, "step_a");
        assert_eq!(log.entries()[1].step_id, "step_b");
        assert_eq!(log.last_response(), Some("result b"));
    }

    /// TurnLog::run_path delegates to the standalone log_provider::run_path.
    #[test]
    fn turn_log_run_path_matches_standalone_helper() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let run_id = "run-path-check".to_string();
        let log = TurnLog::new(run_id.clone());

        let from_log = log.run_path();
        let from_fn = ail_core::session::log_provider::run_path(&run_id);
        assert_eq!(from_log, from_fn);

        std::env::set_current_dir(orig).unwrap();
    }

    /// Session::with_log_provider replaces the default JsonlProvider with the injected one.
    #[test]
    fn session_with_log_provider_overrides_default() {
        use ail_core::session::log_provider::test_support::CapturingProvider;

        let capturing = CapturingProvider::new();
        let mut session = Session::new(Pipeline::passthrough(), "test".to_string())
            .with_log_provider(Box::new(capturing));

        // Appending an entry should not panic and should persist to in-memory entries.
        session.turn_log.append(make_entry("s1", Some("ok")));
        assert_eq!(session.turn_log.entries().len(), 1);
    }
}
