mod executor {
    use ail_core::config::domain::{Pipeline, Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::session::Session;

    fn prompt_step(id: &str, text: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt(text.to_string()),
            tools: None,
            model: None,
            on_result: None,
            runner: None,
        }
    }

    /// SPEC §4.2 — core invariant: steps execute in declared order
    #[test]
    fn steps_execute_in_declaration_order() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let pipeline = Pipeline {
            steps: vec![prompt_step("first", "A"), prompt_step("second", "B")],
            defaults: Default::default(),
            source: None,
        };
        let mut session = Session::new(pipeline, "p".to_string());
        execute(&mut session, &StubRunner::new("r")).unwrap();

        let ids: Vec<_> = session
            .turn_log
            .entries()
            .iter()
            .map(|e| e.step_id.as_str())
            .collect();
        assert_eq!(ids, vec!["first", "second"]);

        std::env::set_current_dir(orig).unwrap();
    }

    /// SPEC §4.1, §4.2 — passthrough runs the invocation step declared at step zero
    #[test]
    fn passthrough_pipeline_runs_invocation_step() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = Session::new(Pipeline::passthrough(), "p".to_string());
        let result = execute(&mut session, &StubRunner::new("x"));
        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "invocation");

        std::env::set_current_dir(orig).unwrap();
    }
}

mod session {
    use ail_core::config::domain::Pipeline;
    use ail_core::session::{Session, TurnEntry, TurnLog};
    use std::time::SystemTime;

    fn make_session() -> Session {
        Session::new(Pipeline::passthrough(), "test prompt".to_string())
    }

    fn make_entry(step_id: &str, response: Option<&str>) -> TurnEntry {
        TurnEntry {
            step_id: step_id.to_string(),
            prompt: "some prompt".to_string(),
            response: response.map(|s| s.to_string()),
            timestamp: SystemTime::now(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            runner_session_id: None,
            stdout: None,
            stderr: None,
            exit_code: None,
        }
    }

    /// SPEC §4 — each pipeline run has a unique run_id
    #[test]
    fn session_new_generates_unique_run_id() {
        let s = make_session();
        assert!(!s.run_id.is_empty());
    }

    /// SPEC §4 — entries are ordered and retrievable
    #[test]
    fn turn_log_entries_are_ordered() {
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut log = TurnLog::new("test-run-ordered".to_string());
        log.append(make_entry("step_1", Some("response 1")));
        log.append(make_entry("step_2", Some("response 2")));
        let entries = log.entries();
        assert_eq!(entries[0].step_id, "step_1");
        assert_eq!(entries[1].step_id, "step_2");

        std::env::set_current_dir(original_dir).unwrap();
    }

    /// SPEC §4 — last_response returns the most recent entry
    #[test]
    fn last_response_returns_most_recent_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut log = TurnLog::new("test-run-last".to_string());
        log.append(make_entry("step_1", Some("first")));
        log.append(make_entry("step_2", Some("second")));
        assert_eq!(log.last_response(), Some("second"));

        std::env::set_current_dir(original_dir).unwrap();
    }

    /// SPEC §4, §4.4 — turn log persists to project-scoped NDJSON file
    #[test]
    fn turn_log_append_writes_ndjson_line_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let run_id = "test-run-ndjson".to_string();
        let mut log = TurnLog::new(run_id.clone());
        log.append(make_entry("step_1", Some("hello")));

        // Path is now project-scoped: ~/.ail/projects/<sha1_of_cwd>/runs/<run_id>.jsonl
        let path = log.run_path();
        assert!(path.exists(), "NDJSON file should exist at {path:?}");
        let contents = std::fs::read_to_string(&path).unwrap();
        // First line may be a step_started event; find the TurnEntry line
        let entry_line = contents
            .lines()
            .find(|l| l.contains("\"step_id\"") && !l.contains("\"type\""))
            .expect("expected a TurnEntry line in NDJSON");
        let parsed: serde_json::Value = serde_json::from_str(entry_line).unwrap();
        assert_eq!(parsed["step_id"], "step_1");

        std::env::set_current_dir(original_dir).unwrap();
    }

    /// SPEC §4 — two sessions produce different run_ids
    #[test]
    fn two_sessions_have_distinct_run_ids() {
        let s1 = make_session();
        let s2 = make_session();
        assert_ne!(s1.run_id, s2.run_id);
    }
}
