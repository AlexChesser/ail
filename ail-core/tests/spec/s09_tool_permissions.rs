mod s9_tool_permissions {
    use ail_core::config::load;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    /// SPEC §5.6 — tools.allow and tools.deny parse from YAML into domain
    #[test]
    fn tool_policy_parses_from_yaml() {
        let pipeline = load(&fixtures_dir().join("tool_permissions.ail.yaml")).unwrap();
        let invocation = &pipeline.steps[0];
        let tools = invocation
            .tools
            .as_ref()
            .expect("invocation should have tools");
        assert!(tools.allow.contains(&"Read".to_string()));
        assert!(tools.allow.contains(&"Edit(./src/*)".to_string()));
        assert!(tools.deny.contains(&"Bash".to_string()));
        assert!(tools.deny.contains(&"WebFetch".to_string()));
    }

    /// SPEC §5.6 — step without tools declaration has no tool policy
    #[test]
    fn step_without_tools_has_no_policy() {
        let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
        assert!(pipeline.steps[0].tools.is_none());
    }

    /// SPEC §5.6 — tool policy is per-step; different steps can have different policies
    #[test]
    fn tool_policy_is_per_step() {
        let pipeline = load(&fixtures_dir().join("tool_permissions.ail.yaml")).unwrap();
        let invocation = &pipeline.steps[0];
        let review = &pipeline.steps[1];
        // invocation has 3 allowed tools, review has 1
        assert_eq!(invocation.tools.as_ref().unwrap().allow.len(), 3);
        assert_eq!(review.tools.as_ref().unwrap().allow.len(), 1);
        assert!(review.tools.as_ref().unwrap().deny.is_empty());
    }

    /// SPEC §3.2 — default_tools applies when a step has no per-step tools
    #[test]
    fn default_tools_applies_when_step_has_no_tools() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        use ail_core::config::domain::{Pipeline, Step, StepBody, StepId, ToolPolicy};
        use ail_core::executor::execute;
        use ail_core::runner::stub::RecordingStubRunner;
        use ail_core::session::Session;

        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let pipeline = Pipeline {
            steps: vec![Step {
                id: StepId("step_a".to_string()),
                body: StepBody::Prompt("do something".to_string()),
                message: None,
                tools: None, // No per-step tools
                model: None,
                on_result: None,
                runner: None,
                condition: None,
                append_system_prompt: None,
                system_prompt: None,
                resume: false,
            }],
            defaults: Default::default(),
            source: None,
            timeout_seconds: None,
            default_tools: Some(ToolPolicy {
                disabled: false,
                allow: vec!["Bash".to_string()],
                deny: vec![],
            }),
            named_pipelines: Default::default(),
        };
        let mut session = Session::new(pipeline, "p".to_string());
        let runner = RecordingStubRunner::new("ok");
        let result = execute(&mut session, &runner);
        assert!(result.is_ok());

        // Verify the runner received an allowlist with Bash
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        use ail_core::runner::ToolPermissionPolicy;
        match &calls[0].tool_policy {
            ToolPermissionPolicy::Allowlist(tools) => {
                assert!(
                    tools.contains(&"Bash".to_string()),
                    "default tools should include Bash"
                );
            }
            other => panic!("expected Allowlist, got {:?}", other),
        }

        std::env::set_current_dir(orig).unwrap();
    }

    /// SPEC §3.2 — per-step tools override default_tools entirely (no merge)
    #[test]
    fn per_step_tools_override_default_tools_entirely() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        use ail_core::config::domain::{Pipeline, Step, StepBody, StepId, ToolPolicy};
        use ail_core::executor::execute;
        use ail_core::runner::stub::RecordingStubRunner;
        use ail_core::session::Session;

        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let pipeline = Pipeline {
            steps: vec![Step {
                id: StepId("step_b".to_string()),
                body: StepBody::Prompt("do something".to_string()),
                message: None,
                tools: Some(ToolPolicy {
                    disabled: false,
                    allow: vec!["Edit".to_string()],
                    deny: vec![],
                }),
                model: None,
                on_result: None,
                runner: None,
                condition: None,
                append_system_prompt: None,
                system_prompt: None,
                resume: false,
            }],
            defaults: Default::default(),
            source: None,
            timeout_seconds: None,
            default_tools: Some(ToolPolicy {
                disabled: false,
                allow: vec!["Bash".to_string()],
                deny: vec![],
            }),
            named_pipelines: Default::default(),
        };
        let mut session = Session::new(pipeline, "p".to_string());
        let runner = RecordingStubRunner::new("ok");
        let result = execute(&mut session, &runner);
        assert!(result.is_ok());

        // Verify the runner received only the per-step tool (Edit), not the default (Bash)
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        use ail_core::runner::ToolPermissionPolicy;
        match &calls[0].tool_policy {
            ToolPermissionPolicy::Allowlist(tools) => {
                assert!(
                    tools.contains(&"Edit".to_string()),
                    "per-step tools should include Edit"
                );
                assert!(
                    !tools.contains(&"Bash".to_string()),
                    "default tools should NOT bleed through"
                );
            }
            other => panic!("expected Allowlist, got {:?}", other),
        }

        std::env::set_current_dir(orig).unwrap();
    }

    /// SPEC §5.6 — InvokeOptions carries tool lists to runner
    #[test]
    fn invoke_options_carries_tool_policy_to_runner() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        use ail_core::config::domain::{Pipeline, Step, StepBody, StepId, ToolPolicy};
        use ail_core::executor::execute;
        use ail_core::runner::stub::StubRunner;
        use ail_core::session::Session;

        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        // Build a pipeline step with a tool policy
        let pipeline = Pipeline {
            steps: vec![Step {
                id: StepId("guarded".to_string()),
                body: StepBody::Prompt("do something".to_string()),
                message: None,
                tools: Some(ToolPolicy {
                    disabled: false,
                    allow: vec!["Read".to_string(), "Edit".to_string()],
                    deny: vec!["Bash".to_string()],
                }),
                model: None,
                on_result: None,
                runner: None,
                condition: None,
                append_system_prompt: None,
                system_prompt: None,
                resume: false,
            }],
            defaults: Default::default(),
            timeout_seconds: None,
            source: None,
            default_tools: None,
            named_pipelines: Default::default(),
        };
        let mut session = Session::new(pipeline, "p".to_string());
        // StubRunner ignores tool policy (it's a test double); we verify it doesn't error
        let result = execute(&mut session, &StubRunner::new("ok"));
        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries()[0].step_id, "guarded");

        std::env::set_current_dir(orig).unwrap();
    }
}
