/// SPEC §26 — output_schema validation and input_schema reserved field tests.

mod parse_valid {
    use ail_core::config;
    use ail_core::config::domain::StepBody;

    /// §26 — output_schema is accepted and stored on the domain Step.
    #[test]
    fn output_schema_parses_successfully() {
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
      required: [name]
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        assert_eq!(pipeline.steps.len(), 1);
        assert!(
            pipeline.steps[0].output_schema.is_some(),
            "output_schema should be present on the step"
        );
        let schema = pipeline.steps[0].output_schema.as_ref().unwrap();
        assert_eq!(schema["type"], "object");
    }

    /// §26 — output_schema with type: array parses.
    #[test]
    fn output_schema_array_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: plan
    prompt: "plan tasks"
    output_schema:
      type: array
      items:
        type: string
      maxItems: 20
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        let schema = pipeline.steps[0].output_schema.as_ref().unwrap();
        assert_eq!(schema["type"], "array");
    }

    /// §26 — step without output_schema has None.
    #[test]
    fn step_without_output_schema_has_none() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: plain
    prompt: "hello"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        assert!(pipeline.steps[0].output_schema.is_none());
    }

    /// §26 — output_schema works with context:shell: steps too.
    #[test]
    fn output_schema_on_context_step() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: json_cmd
    context:
      shell: "echo '{\"status\": \"ok\"}'"
    output_schema:
      type: object
      properties:
        status:
          type: string
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        assert!(pipeline.steps[0].output_schema.is_some());
        match &pipeline.steps[0].body {
            StepBody::Context(_) => {} // expected
            other => panic!("Expected Context step, got {other:?}"),
        }
    }
}

mod parse_invalid {
    use ail_core::config;
    use ail_core::error::error_types;

    /// §26 — input_schema is still rejected as reserved.
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

mod executor {
    use ail_core::config::domain::{ContextSource, Step, StepBody, StepId};
    use ail_core::error::error_types;
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::test_helpers::make_session;

    /// §26 — valid JSON response passes output_schema validation.
    #[test]
    fn valid_json_passes_schema_validation() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "category": { "type": "string" }
            },
            "required": ["category"]
        });
        let step = Step {
            id: StepId("classify".to_string()),
            body: StepBody::Prompt("classify".to_string()),
            output_schema: Some(schema),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new(r#"{"category": "bugfix"}"#);
        let result = execute(&mut session, &runner);
        assert!(result.is_ok(), "Valid JSON should pass schema: {result:?}");
    }

    /// §26 — non-JSON response fails output_schema validation.
    #[test]
    fn non_json_response_fails_validation() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object"
        });
        let step = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            output_schema: Some(schema),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("not json at all");
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// §26 — JSON that doesn't match schema fails validation.
    #[test]
    fn json_not_matching_schema_fails_validation() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "priority": { "type": "integer", "minimum": 1, "maximum": 5 }
            },
            "required": ["priority"]
        });
        let step = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            output_schema: Some(schema),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        // Missing required field "priority"
        let runner = StubRunner::new(r#"{"category": "bugfix"}"#);
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// §26 — step without output_schema skips validation.
    #[test]
    fn step_without_schema_skips_validation() {
        let step = Step {
            id: StepId("plain".to_string()),
            body: StepBody::Prompt("hello".to_string()),
            output_schema: None,
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("just plain text, not JSON");
        assert!(execute(&mut session, &runner).is_ok());
    }

    /// §26 — output_schema validation works on context:shell: steps.
    #[test]
    fn schema_validation_on_context_step() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "string" }
            },
            "required": ["status"]
        });
        let step = Step {
            id: StepId("cmd".to_string()),
            body: StepBody::Context(ContextSource::Shell(
                r#"echo '{"status": "ok"}'"#.to_string(),
            )),
            output_schema: Some(schema),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        // Context steps produce stdout, not response. The schema validation
        // checks the response field, which is None for context steps.
        // This should fail since there's no JSON response to validate.
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }
}

mod template {
    use ail_core::config::domain::{Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::test_helpers::make_session;

    /// §26.5 — {{ step.<id>.items }} resolves for array responses.
    #[test]
    fn items_template_variable_resolves_array() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "array",
            "items": { "type": "string" }
        });
        let producer = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan tasks".to_string()),
            output_schema: Some(schema),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("use".to_string()),
            body: StepBody::Prompt("Items: {{ step.plan.items }}".to_string()),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        let runner = StubRunner::new(r#"["task1","task2","task3"]"#);
        execute(&mut session, &runner).unwrap();

        let use_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "use")
            .expect("'use' step should exist");
        assert!(
            use_entry.prompt.contains("task1"),
            "Items should be resolved in prompt, got: {}",
            use_entry.prompt
        );
    }

    /// §26.5 — {{ step.<id>.items }} fails for non-array responses.
    #[test]
    fn items_template_fails_for_non_array() {
        let producer = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("use".to_string()),
            body: StepBody::Prompt("Items: {{ step.gen.items }}".to_string()),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        let runner = StubRunner::new(r#"{"not": "an array"}"#);
        let result = execute(&mut session, &runner);
        assert!(result.is_err(), "Should fail for non-array: {result:?}");
    }

    /// §26.5 — {{ step.<id>.items }} fails for non-JSON responses.
    #[test]
    fn items_template_fails_for_non_json() {
        let producer = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("use".to_string()),
            body: StepBody::Prompt("Items: {{ step.gen.items }}".to_string()),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        let runner = StubRunner::new("plain text");
        let result = execute(&mut session, &runner);
        assert!(result.is_err(), "Should fail for non-JSON: {result:?}");
    }
}
