/// SPEC §26 — output_schema and input_schema are reserved fields rejected at parse time
/// until implementation is complete.

mod reserved_field_rejection {
    use ail_core::config;
    use ail_core::error::error_types;

    /// §26 — output_schema is rejected with a clear "reserved" error.
    #[test]
    fn output_schema_rejected_as_reserved() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: gen
    prompt: "generate JSON"
    output_schema:
      type: object
      properties:
        name:
          type: string
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(result.is_err(), "output_schema must be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("output_schema"),
            "Error should mention the field name, got: {}",
            err.detail()
        );
        assert!(
            err.detail().contains("reserved"),
            "Error should mention 'reserved', got: {}",
            err.detail()
        );
    }

    /// §26 — input_schema is rejected with a clear "reserved" error.
    #[test]
    fn input_schema_rejected_as_reserved() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: validate
    prompt: "validate input"
    input_schema:
      type: object
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(result.is_err(), "input_schema must be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("input_schema"),
            "Error should mention the field name, got: {}",
            err.detail()
        );
    }
}
