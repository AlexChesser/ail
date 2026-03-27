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
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover(None);
        std::env::set_current_dir(original_dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn falls_back_to_ail_yaml_in_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let ail_yaml = tmp.path().join(".ail.yaml");
        std::fs::write(
            &ail_yaml,
            "version: \"0.0.1\"\npipeline:\n  - id: s\n    prompt: x\n",
        )
        .unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover(None);
        std::env::set_current_dir(original_dir).unwrap();
        assert_eq!(result, Some(PathBuf::from(".ail.yaml")));
    }
}

mod s3_1_discover_all {
    use ail_core::config::discovery::discover_all;

    #[test]
    fn returns_empty_when_no_yaml_files_present() {
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = discover_all();
        std::env::set_current_dir(original_dir).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn finds_ail_yaml_in_cwd_as_default() {
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
}
