mod s5_9_append_system_prompt {
    use ail_core::config::domain::SystemPromptEntry;
    use ail_core::config::load;

    /// SPEC §5.9 — bare string in append_system_prompt parses as SystemPromptEntry::Text
    #[test]
    fn append_system_prompt_bare_string_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: review
    prompt: "review this"
    append_system_prompt:
      - "Some context"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = load(tmp.path()).unwrap();
        let entries = pipeline.steps[0].append_system_prompt.as_ref().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            SystemPromptEntry::Text("Some context".to_string())
        );
    }

    /// SPEC §5.9 — structured entries (text:, file:, shell:) all parse correctly
    #[test]
    fn append_system_prompt_structured_entries_parse() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: review
    prompt: "review this"
    append_system_prompt:
      - text: "header"
      - file: "./context.md"
      - shell: "echo context"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = load(tmp.path()).unwrap();
        let entries = pipeline.steps[0].append_system_prompt.as_ref().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], SystemPromptEntry::Text("header".to_string()));
        assert_eq!(
            entries[1],
            SystemPromptEntry::File(std::path::PathBuf::from("./context.md"))
        );
        assert_eq!(
            entries[2],
            SystemPromptEntry::Shell("echo context".to_string())
        );
    }

    /// SPEC §5.9 — a structured entry with no keys set returns CONFIG_VALIDATION_FAILED
    #[test]
    fn append_system_prompt_invalid_structured_entry_is_error() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: review
    prompt: "review this"
    append_system_prompt:
      - {}
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = load(tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            ail_core::error::error_types::CONFIG_VALIDATION_FAILED
        );
        assert!(
            err.detail().contains("append_system_prompt"),
            "Expected error detail to mention append_system_prompt, got: {}",
            err.detail()
        );
    }
}

mod s5_2_file_path_resolution {
    use ail_core::executor::execute;
    use ail_core::runner::stub::EchoStubRunner;
    use ail_core::session::Session;

    /// SPEC §5.2 — `./` in `system_prompt:` is resolved relative to the pipeline file,
    /// not the process working directory.
    #[test]
    fn system_prompt_resolves_relative_to_pipeline_file() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join("prompts");
        std::fs::create_dir(&prompts_dir).unwrap();

        // Write the system prompt in a sub-directory relative to the pipeline file.
        let sp_path = prompts_dir.join("system.md");
        std::fs::write(&sp_path, "You are a test agent.").unwrap();

        let yaml = "version: \"0.0.1\"\npipeline:\n  - id: invocation\n    system_prompt: ./prompts/system.md\n    prompt: \"{{ step.invocation.prompt }}\"\n";
        let pipeline_path = tmp.path().join("test.ail.yaml");
        std::fs::write(&pipeline_path, yaml).unwrap();

        let pipeline = ail_core::config::load(&pipeline_path).unwrap();
        let mut session = Session::new(pipeline, "test prompt".to_string());

        // Change CWD to somewhere else so we confirm resolution is NOT CWD-relative.
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(std::env::temp_dir()).unwrap();

        let runner = EchoStubRunner::new();
        let result = execute(&mut session, &runner);

        std::env::set_current_dir(orig).unwrap();

        assert!(
            result.is_ok(),
            "Expected Ok — system_prompt should resolve relative to pipeline file, got: {result:?}"
        );
    }

    /// SPEC §5.2 — `./` in `prompt:` is resolved relative to the pipeline file.
    #[test]
    fn prompt_file_resolves_relative_to_pipeline_file() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join("prompts");
        std::fs::create_dir(&prompts_dir).unwrap();

        let prompt_path = prompts_dir.join("task.md");
        std::fs::write(&prompt_path, "Do the task.").unwrap();

        let yaml = "version: \"0.0.1\"\npipeline:\n  - id: step\n    prompt: ./prompts/task.md\n";
        let pipeline_path = tmp.path().join("test2.ail.yaml");
        std::fs::write(&pipeline_path, yaml).unwrap();

        let pipeline = ail_core::config::load(&pipeline_path).unwrap();
        let mut session = Session::new(pipeline, "invocation prompt".to_string());

        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(std::env::temp_dir()).unwrap();

        let runner = EchoStubRunner::new();
        let result = execute(&mut session, &runner);

        std::env::set_current_dir(orig).unwrap();

        assert!(
            result.is_ok(),
            "Expected Ok — prompt file should resolve relative to pipeline file, got: {result:?}"
        );
        // The echo runner returns the resolved prompt text.
        let response = session.turn_log.last_response().unwrap_or("");
        assert_eq!(response, "Do the task.");
    }
}

mod s5_1_core_fields {
    use ail_core::config::{domain::StepBody, load};
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn prompt_field_parses_to_prompt_body() {
        let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
        assert!(matches!(pipeline.steps[0].body, StepBody::Prompt(_)));
    }

    #[test]
    fn step_id_is_newtype_not_raw_string() {
        let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
        // StepId is a newtype — verify we access it via .as_str()
        assert_eq!(pipeline.steps[0].id.as_str(), "dont_be_stupid");
    }

    #[test]
    fn duplicate_step_ids_return_validation_error() {
        let result = load(&fixtures_dir().join("invalid_duplicate_ids.ail.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("review"));
    }

    #[test]
    fn step_with_no_primary_field_is_invalid() {
        let result = load(&fixtures_dir().join("invalid_no_primary_field.ail.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("primary field"));
    }

    /// SPEC §4.1 — invocation step must be first if declared
    #[test]
    fn invocation_step_not_first_is_invalid() {
        let result = load(&fixtures_dir().join("invalid_invocation_not_first.ail.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invocation"));
    }

    /// SPEC §4.1 — invocation declared as first step is valid
    #[test]
    fn invocation_step_declared_first_is_valid() {
        use ail_core::config::domain::Pipeline;
        // passthrough() declares invocation as step zero — it must load/use cleanly
        let p = Pipeline::passthrough();
        assert_eq!(p.steps[0].id.as_str(), "invocation");
    }
}
