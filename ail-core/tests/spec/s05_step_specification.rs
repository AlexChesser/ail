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
