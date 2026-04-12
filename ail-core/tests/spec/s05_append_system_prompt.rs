//! Specification §5.9: append_system_prompt — integration tests verifying that
//! Text, File, and Shell entries are resolved and forwarded to the runner via InvokeOptions.
//!
//! These tests drive the full headless executor path with RecordingStubRunner to capture
//! what was actually passed as `append_system_prompt` on each runner invocation.

mod append_system_prompt {
    use ail_core::config::domain::{Pipeline, Step, StepBody, StepId, SystemPromptEntry};
    use ail_core::executor::execute;
    use ail_core::runner::stub::RecordingStubRunner;
    use ail_core::session::log_provider::NullProvider;
    use ail_core::session::Session;

    fn make_step_with_append(id: &str, entries: Vec<SystemPromptEntry>) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt("say hi".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: Some(entries),
            system_prompt: None,
            resume: false,
            on_error: None,
        }
    }

    fn make_session(steps: Vec<Step>) -> Session {
        let pipeline = Pipeline {
            steps,
            source: None,
            defaults: Default::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        Session::new(pipeline, "test prompt".to_string()).with_log_provider(Box::new(NullProvider))
    }

    /// Inline Text entries are forwarded verbatim to the runner's append_system_prompt.
    #[test]
    fn text_entry_reaches_runner_invoke_options() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let step = make_step_with_append(
            "hello",
            vec![SystemPromptEntry::Text("inline system context".to_string())],
        );
        let runner = RecordingStubRunner::new("ok");
        let mut session = make_session(vec![step]);
        execute(&mut session, &runner).unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].append_system_prompt, vec!["inline system context"]);

        std::env::set_current_dir(orig).unwrap();
    }

    /// File entries are read at step runtime and forwarded as resolved text.
    #[test]
    fn file_entry_reaches_runner_with_file_content() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let prompt_file = tmp.path().join("sys_ctx.txt");
        std::fs::write(&prompt_file, "system context from file").unwrap();

        let step = make_step_with_append("hello", vec![SystemPromptEntry::File(prompt_file)]);
        let runner = RecordingStubRunner::new("ok");
        let mut session = make_session(vec![step]);
        execute(&mut session, &runner).unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].append_system_prompt,
            vec!["system context from file"]
        );

        std::env::set_current_dir(orig).unwrap();
    }

    /// Shell entries are executed and their stdout forwarded as resolved text.
    #[test]
    fn shell_entry_reaches_runner_with_stdout() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let step = make_step_with_append(
            "hello",
            vec![SystemPromptEntry::Shell("echo 'from shell'".to_string())],
        );
        let runner = RecordingStubRunner::new("ok");
        let mut session = make_session(vec![step]);
        execute(&mut session, &runner).unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].append_system_prompt.len(), 1);
        assert!(
            calls[0].append_system_prompt[0].contains("from shell"),
            "expected shell stdout in append_system_prompt, got: {:?}",
            calls[0].append_system_prompt[0]
        );

        std::env::set_current_dir(orig).unwrap();
    }

    /// Multiple entries are all resolved and forwarded in declaration order.
    #[test]
    fn multiple_entries_forwarded_in_order() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let file = tmp.path().join("ctx.txt");
        std::fs::write(&file, "from file").unwrap();

        let step = make_step_with_append(
            "hello",
            vec![
                SystemPromptEntry::Text("first".to_string()),
                SystemPromptEntry::File(file),
                SystemPromptEntry::Shell("echo 'third'".to_string()),
            ],
        );
        let runner = RecordingStubRunner::new("ok");
        let mut session = make_session(vec![step]);
        execute(&mut session, &runner).unwrap();

        let calls = runner.calls();
        assert_eq!(calls[0].append_system_prompt.len(), 3);
        assert_eq!(calls[0].append_system_prompt[0], "first");
        assert_eq!(calls[0].append_system_prompt[1], "from file");
        assert!(calls[0].append_system_prompt[2].contains("third"));

        std::env::set_current_dir(orig).unwrap();
    }

    /// When no append_system_prompt is declared the runner receives an empty Vec.
    #[test]
    fn no_append_entries_runner_receives_empty_vec() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let step = Step {
            id: StepId("plain".to_string()),
            body: StepBody::Prompt("hello".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            on_error: None,
        };
        let runner = RecordingStubRunner::new("ok");
        let mut session = make_session(vec![step]);
        execute(&mut session, &runner).unwrap();

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].append_system_prompt.is_empty(),
            "expected empty append_system_prompt for step without entries"
        );

        std::env::set_current_dir(orig).unwrap();
    }
}
