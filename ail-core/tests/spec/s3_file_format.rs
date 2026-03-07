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
