/// SPEC §27 — do_while: bounded repeat-until loop validation tests.

mod parse_valid {
    use ail_core::config;
    use ail_core::config::domain::{ConditionOp, StepBody};

    /// §27 — valid do_while config parses successfully.
    #[test]
    fn valid_do_while_parses() {
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
            shell: "cargo test"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        assert_eq!(pipeline.steps.len(), 1);
        match &pipeline.steps[0].body {
            StepBody::DoWhile {
                max_iterations,
                exit_when,
                steps,
            } => {
                assert_eq!(*max_iterations, 5);
                assert_eq!(exit_when.op, ConditionOp::Eq);
                assert!(exit_when.lhs.contains("step.check.exit_code"));
                assert_eq!(exit_when.rhs, "0");
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].id.as_str(), "check");
            }
            other => panic!("Expected DoWhile, got {other:?}"),
        }
    }

    /// §27 — do_while with max_iterations: 1 is valid (minimum).
    #[test]
    fn max_iterations_one_is_valid() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: once_loop
    do_while:
      max_iterations: 1
      exit_when: "{{ step.check.exit_code }} == 0"
      steps:
        - id: check
          context:
            shell: "echo done"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        match &pipeline.steps[0].body {
            StepBody::DoWhile { max_iterations, .. } => assert_eq!(*max_iterations, 1),
            other => panic!("Expected DoWhile, got {other:?}"),
        }
    }

    /// §27 — do_while with multiple inner steps parses.
    #[test]
    fn multiple_inner_steps_parse() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: multi_loop
    do_while:
      max_iterations: 3
      exit_when: "{{ step.verify.exit_code }} == 0"
      steps:
        - id: fix
          prompt: "Fix the issue"
        - id: verify
          context:
            shell: "cargo test"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        match &pipeline.steps[0].body {
            StepBody::DoWhile { steps, .. } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].id.as_str(), "fix");
                assert_eq!(steps[1].id.as_str(), "verify");
            }
            other => panic!("Expected DoWhile, got {other:?}"),
        }
    }

    /// §27 — do_while with word-based exit_when operator.
    #[test]
    fn exit_when_contains_operator() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: check_loop
    do_while:
      max_iterations: 10
      exit_when: "{{ step.review.response }} contains LGTM"
      steps:
        - id: review
          prompt: "Review the code"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        match &pipeline.steps[0].body {
            StepBody::DoWhile { exit_when, .. } => {
                assert_eq!(exit_when.op, ConditionOp::Contains);
                assert_eq!(exit_when.rhs, "LGTM");
            }
            other => panic!("Expected DoWhile, got {other:?}"),
        }
    }
}

mod parse_invalid {
    use ail_core::config;
    use ail_core::error::error_types;

    /// §27 — missing max_iterations is rejected.
    #[test]
    fn missing_max_iterations_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad_loop
    do_while:
      exit_when: "{{ step.check.exit_code }} == 0"
      steps:
        - id: check
          context:
            shell: "echo ok"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("max_iterations"),
            "Expected error about max_iterations, got: {}",
            err.detail()
        );
    }

    /// §27 — max_iterations: 0 is rejected.
    #[test]
    fn zero_max_iterations_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad_loop
    do_while:
      max_iterations: 0
      exit_when: "{{ step.check.exit_code }} == 0"
      steps:
        - id: check
          context:
            shell: "echo ok"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("max_iterations"),
            "Expected error about max_iterations, got: {}",
            err.detail()
        );
    }

    /// §27 — missing exit_when is rejected.
    #[test]
    fn missing_exit_when_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad_loop
    do_while:
      max_iterations: 5
      steps:
        - id: check
          context:
            shell: "echo ok"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("exit_when"),
            "Expected error about exit_when, got: {}",
            err.detail()
        );
    }

    /// §27 — missing steps is rejected.
    #[test]
    fn missing_steps_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad_loop
    do_while:
      max_iterations: 5
      exit_when: "{{ step.check.exit_code }} == 0"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("steps"),
            "Expected error about steps, got: {}",
            err.detail()
        );
    }

    /// §27 — empty steps array is rejected.
    #[test]
    fn empty_steps_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad_loop
    do_while:
      max_iterations: 5
      exit_when: "{{ step.check.exit_code }} == 0"
      steps: []
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("steps"),
            "Expected error about steps, got: {}",
            err.detail()
        );
    }

    /// §27 — do_while + prompt on same step is primary field conflict.
    #[test]
    fn do_while_plus_prompt_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: conflict
    prompt: "hello"
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
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("primary field"),
            "Expected primary field conflict error, got: {}",
            err.detail()
        );
    }

    /// §27 — exit_when with invalid expression syntax is rejected.
    #[test]
    fn invalid_exit_when_expression_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad_loop
    do_while:
      max_iterations: 5
      exit_when: "not a valid expression"
      steps:
        - id: check
          context:
            shell: "echo ok"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    /// §27 — duplicate inner step IDs are rejected.
    #[test]
    fn duplicate_inner_step_ids_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: dup_loop
    do_while:
      max_iterations: 3
      exit_when: "{{ step.check.exit_code }} == 0"
      steps:
        - id: check
          context:
            shell: "echo ok"
        - id: check
          prompt: "duplicate"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("check"),
            "Expected error about duplicate id 'check', got: {}",
            err.detail()
        );
    }

    /// §27 — inner step missing id is rejected.
    #[test]
    fn inner_step_missing_id_rejected() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: no_id_loop
    do_while:
      max_iterations: 3
      exit_when: "{{ step.check.exit_code }} == 0"
      steps:
        - prompt: "no id here"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("id"),
            "Expected error about missing id, got: {}",
            err.detail()
        );
    }
}

mod materialize {
    use ail_core::config;
    use ail_core::materialize::materialize;

    /// §27 — do_while steps appear in materialize output.
    #[test]
    fn do_while_appears_in_materialize() {
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
            shell: "cargo test"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        let output = materialize(&pipeline);
        assert!(
            output.contains("do_while:"),
            "do_while: missing from materialize output"
        );
        assert!(
            output.contains("max_iterations: 5"),
            "max_iterations missing"
        );
        assert!(output.contains("exit_when:"), "exit_when missing");
        assert!(output.contains("check"), "inner step id missing");
        // Verify output is valid YAML.
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
        assert!(
            result.is_ok(),
            "Materialize output is not valid YAML: {output}"
        );
    }
}
