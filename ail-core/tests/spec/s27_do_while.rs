/// SPEC §27 — do_while is a reserved field rejected at parse time
/// until implementation is complete.

mod reserved_field_rejection {
    use ail_core::config;
    use ail_core::error::error_types;

    /// §27 — do_while is rejected with a clear "reserved" error.
    #[test]
    fn do_while_rejected_as_reserved() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: retry_loop
    do_while:
      max_iterations: 5
      exit_when: "{{ step.check.exit_code }} == 0"
      steps:
        - id: check
          context:
            shell: "echo ok"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(result.is_err(), "do_while must be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("do_while"),
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
