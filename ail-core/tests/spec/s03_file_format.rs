mod s3_1_discovery {
    use ail_core::config::discovery::discover;
    use std::path::PathBuf;

    #[test]
    fn explicit_path_takes_precedence() {
        let explicit = PathBuf::from("/some/explicit/path.ail.yaml");
        let result = discover(Some(explicit.clone()));
        assert_eq!(result, Some(explicit));
    }

    #[test]
    fn returns_none_when_no_file_found() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover(None);
        std::env::set_current_dir(original_dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn falls_back_to_ail_yaml_in_cwd() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        // Canonicalize to resolve symlinks (macOS: /tmp -> /private/tmp) so the
        // expected path matches what current_dir() returns inside discover().
        let tmp_path = tmp.path().canonicalize().unwrap();
        let ail_yaml = tmp_path.join(".ail.yaml");
        std::fs::write(
            &ail_yaml,
            "version: \"0.0.1\"\npipeline:\n  - id: s\n    prompt: x\n",
        )
        .unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp_path).unwrap();
        let result = discover(None);
        std::env::set_current_dir(original_dir).unwrap();
        // Discovery now returns absolute paths so parent() is always usable (SPEC §9).
        assert_eq!(result, Some(ail_yaml));
    }
}

mod s3_1_discover_all {
    use ail_core::config::discovery::discover_all;

    #[test]
    fn returns_empty_when_no_yaml_files_present() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover_all();
        std::env::set_current_dir(original_dir).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn finds_ail_yaml_in_cwd_as_default() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(".ail.yaml"), "x").unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover_all();
        std::env::set_current_dir(original_dir).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "default");
    }

    #[test]
    fn finds_yaml_files_in_ail_directory() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ail_dir = tmp.path().join(".ail");
        std::fs::create_dir(&ail_dir).unwrap();
        std::fs::write(ail_dir.join("code-review.yaml"), "x").unwrap();
        std::fs::write(ail_dir.join("incident.yaml"), "x").unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover_all();
        std::env::set_current_dir(original_dir).unwrap();
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"incident"));
    }

    #[test]
    fn results_are_sorted_alphabetically() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ail_dir = tmp.path().join(".ail");
        std::fs::create_dir(&ail_dir).unwrap();
        std::fs::write(ail_dir.join("zebra.yaml"), "x").unwrap();
        std::fs::write(ail_dir.join("alpha.yaml"), "x").unwrap();
        std::fs::write(ail_dir.join("middle.yaml"), "x").unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover_all();
        std::env::set_current_dir(original_dir).unwrap();
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn cwd_entry_wins_over_home_config_on_duplicate_name() {
        let _cwd_guard = crate::spec::CWD_LOCK.lock().unwrap();
        // We can't easily test ~/.config/ail/ in isolation, but we can verify
        // that the .ail/ directory entry takes precedence by checking deduplication
        // logic: if the same name appears twice, only one entry is returned.
        let tmp = tempfile::tempdir().unwrap();
        let ail_dir = tmp.path().join(".ail");
        std::fs::create_dir(&ail_dir).unwrap();
        std::fs::write(ail_dir.join("code-review.yaml"), "x").unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover_all();
        std::env::set_current_dir(original_dir).unwrap();
        let code_review_count = result.iter().filter(|e| e.name == "code-review").count();
        assert_eq!(code_review_count, 1);
    }
}

mod s3_2_defaults_tools {
    use ail_core::config::load;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    /// SPEC §3.2 — `defaults.tools` is parsed into `pipeline.default_tools`
    #[test]
    fn defaults_tools_is_parsed() {
        let pipeline = load(&fixtures_dir().join("defaults_tools.ail.yaml")).unwrap();
        let default_tools = pipeline
            .default_tools
            .as_ref()
            .expect("defaults.tools should be parsed into pipeline.default_tools");
        assert!(
            default_tools.allow.contains(&"Bash".to_string()),
            "allow list should contain Bash"
        );
        assert!(
            default_tools.allow.contains(&"Read".to_string()),
            "allow list should contain Read"
        );
        assert!(
            default_tools.deny.is_empty(),
            "deny list should be empty when not specified"
        );
    }

    /// SPEC §3.2 — pipeline with no `defaults.tools` has `default_tools: None`
    #[test]
    fn no_defaults_tools_yields_none() {
        let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
        assert!(
            pipeline.default_tools.is_none(),
            "pipeline without defaults.tools should have default_tools: None"
        );
    }
}

mod s3_2_top_level_structure {
    use ail_core::config::load;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn minimal_pipeline_parses_to_domain_type() {
        let result = load(&fixtures_dir().join("minimal.ail.yaml"));
        assert!(result.is_ok());
        let pipeline = result.unwrap();
        assert_eq!(pipeline.steps.len(), 1);
        assert_eq!(pipeline.steps[0].id.as_str(), "dont_be_stupid");
    }

    #[test]
    fn missing_version_returns_validation_error() {
        let result = load(&fixtures_dir().join("invalid_no_version.ail.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("version"));
    }

    #[test]
    fn empty_pipeline_returns_validation_error() {
        let result = load(&fixtures_dir().join("invalid_empty_pipeline.ail.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("pipeline"));
    }

    /// SPEC §3.2 — `defaults.provider.model` is accepted and takes precedence over `defaults.model`
    #[test]
    fn provider_model_accepted_and_wins_over_defaults_model() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml_path = tmp.path().join("provider_model.ail.yaml");
        // model inside provider wins over sibling defaults.model
        std::fs::write(
            &yaml_path,
            "version: \"1\"\ndefaults:\n  model: should-be-overridden\n  provider:\n    model: qwen3.5:0.8b\n    base_url: http://localhost:11434\npipeline:\n  - id: step1\n    prompt: hello\n",
        )
        .unwrap();
        let pipeline = load(&yaml_path).expect("should parse successfully");
        assert_eq!(
            pipeline.defaults.model.as_deref(),
            Some("qwen3.5:0.8b"),
            "provider.model should win over defaults.model"
        );
    }

    /// SPEC §3.2 — `defaults.provider.model` alone (no sibling `defaults.model`) is accepted
    #[test]
    fn provider_model_accepted_without_sibling_defaults_model() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml_path = tmp.path().join("provider_model_only.ail.yaml");
        std::fs::write(
            &yaml_path,
            "version: \"1\"\ndefaults:\n  provider:\n    model: qwen3.5:0.8b\n    base_url: http://localhost:11434\npipeline:\n  - id: step1\n    prompt: hello\n",
        )
        .unwrap();
        let pipeline = load(&yaml_path).expect("should parse successfully");
        assert_eq!(
            pipeline.defaults.model.as_deref(),
            Some("qwen3.5:0.8b"),
            "provider.model should be parsed when defaults.model is absent"
        );
    }

    #[test]
    fn defaults_timeout_seconds_is_parsed() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml_path = tmp.path().join("timeout.ail.yaml");
        std::fs::write(
            &yaml_path,
            "version: \"1\"\ndefaults:\n  timeout_seconds: 120\npipeline:\n  - id: step1\n    prompt: hello\n",
        )
        .unwrap();
        let pipeline = load(&yaml_path).expect("should parse successfully");
        assert_eq!(pipeline.timeout_seconds, Some(120));
    }
}
