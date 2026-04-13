/// SPEC §28 — for_each is a reserved field rejected at parse time
/// until implementation is complete.

mod reserved_field_rejection {
    use ail_core::config;
    use ail_core::error::error_types;

    /// §28 — for_each is rejected with a clear "reserved" error.
    #[test]
    fn for_each_rejected_as_reserved() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: process_items
    for_each:
      over: "{{ step.planner.items }}"
      as: task
      steps:
        - id: implement
          prompt: "implement {{ for_each.task }}"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(result.is_err(), "for_each must be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("for_each"),
            "Error should mention the field name, got: {}",
            err.detail()
        );
        assert!(
            err.detail().contains("reserved"),
            "Error should mention 'reserved', got: {}",
            err.detail()
        );
    }
}
