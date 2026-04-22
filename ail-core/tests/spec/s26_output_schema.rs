//! SPEC s26 -- Structured step I/O schemas: output_schema, input_schema,
//! field:equals: operator, and parse-time compatibility checks.

mod parse_valid {
    use ail_core::config;
    use ail_core::config::domain::StepBody;

    /// s26.1 -- output_schema is accepted and stored on the domain Step.
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
        assert!(pipeline.steps[0].output_schema.is_some());
        let schema = pipeline.steps[0].output_schema.as_ref().unwrap();
        assert_eq!(schema["type"], "object");
    }

    /// s26.1 -- output_schema with type: array parses.
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

    /// s26.1 -- step without output_schema has None.
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

    /// s26.1 -- output_schema works with context:shell: steps too.
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

    /// s26.2 -- input_schema is accepted and stored on the domain Step.
    #[test]
    fn input_schema_parses_successfully() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: gen
    prompt: "generate"
  - id: validate
    prompt: "handle input"
    input_schema:
      type: object
      properties:
        category:
          type: string
      required: [category]
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        assert!(pipeline.steps[1].input_schema.is_some());
        let schema = pipeline.steps[1].input_schema.as_ref().unwrap();
        assert_eq!(schema["type"], "object");
    }

    /// s26.2 -- both output_schema and input_schema on the same step.
    #[test]
    fn both_schemas_on_same_step() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: transform
    prompt: "transform"
    output_schema:
      type: object
      properties:
        result:
          type: string
    input_schema:
      type: object
      properties:
        input:
          type: string
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        assert!(pipeline.steps[0].output_schema.is_some());
        assert!(pipeline.steps[0].input_schema.is_some());
    }

    /// s26.4 -- field:equals: on_result format parses with input_schema.
    #[test]
    fn field_equals_on_result_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: classify
    prompt: "classify"
  - id: route
    prompt: "route"
    input_schema:
      type: object
      properties:
        category:
          type: string
      required: [category]
    on_result:
      field: category
      equals: "bugfix"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).unwrap();
        let branches = pipeline.steps[1].on_result.as_ref().unwrap();
        assert_eq!(branches.len(), 2, "field:equals: should produce 2 branches");
    }
}

mod parse_invalid {
    use ail_core::config;
    use ail_core::error::error_types;

    /// s26.2 -- input_schema accepts valid JSON Schema without error.
    #[test]
    fn valid_input_schema_accepted() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: gen
    prompt: "generate"
  - id: consume
    prompt: "consume"
    input_schema:
      type: object
      properties:
        name:
          type: string
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(
            result.is_ok(),
            "Valid input_schema should be accepted: {:?}",
            result.err()
        );
    }

    /// s26.3 -- incompatible adjacent schemas (type mismatch) fails at parse time.
    #[test]
    fn schema_compatibility_type_mismatch() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: producer
    prompt: "produce"
    output_schema:
      type: array
      items:
        type: string
  - id: consumer
    prompt: "consume"
    input_schema:
      type: object
      properties:
        name:
          type: string
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(result.is_err(), "Type mismatch should fail at parse time");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::SCHEMA_COMPATIBILITY_FAILED);
    }

    /// s26.3 -- incompatible adjacent schemas (missing required field) fails at parse time.
    #[test]
    fn schema_compatibility_missing_required_field() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: producer
    prompt: "produce"
    output_schema:
      type: object
      properties:
        name:
          type: string
  - id: consumer
    prompt: "consume"
    input_schema:
      type: object
      properties:
        category:
          type: string
      required: [category]
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(
            result.is_err(),
            "Missing required field should fail at parse time"
        );
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::SCHEMA_COMPATIBILITY_FAILED);
        assert!(err.detail().contains("category"));
    }

    /// s26.3 -- compatible adjacent schemas pass parse-time check.
    #[test]
    fn schema_compatibility_passes_when_compatible() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: producer
    prompt: "produce"
    output_schema:
      type: object
      properties:
        category:
          type: string
        priority:
          type: integer
      required: [category]
  - id: consumer
    prompt: "consume"
    input_schema:
      type: object
      properties:
        category:
          type: string
      required: [category]
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(
            result.is_ok(),
            "Compatible schemas should pass: {:?}",
            result.err()
        );
    }

    /// s26.4 -- field:equals: without input_schema is a parse error.
    #[test]
    fn field_equals_without_input_schema() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: route
    prompt: "route"
    on_result:
      field: category
      equals: "bugfix"
      if_true:
        action: continue
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(
            result.is_err(),
            "field:equals: without input_schema should fail"
        );
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("input_schema"));
    }

    /// s26.4 -- field:equals: referencing a field not in input_schema is a parse error.
    #[test]
    fn field_equals_field_not_in_schema() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: route
    prompt: "route"
    input_schema:
      type: object
      properties:
        name:
          type: string
      required: [name]
    on_result:
      field: category
      equals: "bugfix"
      if_true:
        action: continue
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(
            result.is_err(),
            "Referencing field not in schema should fail"
        );
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("category"));
    }

    /// s26 -- for_each is mutually exclusive with prompt (SPEC §28.2).
    #[test]
    fn for_each_mutually_exclusive_with_prompt() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: each
    prompt: "do things"
    for_each:
      over: "{{ step.plan.items }}"
      steps:
        - id: inner
          prompt: "do"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let result = config::load(tmp.path());
        assert!(result.is_err(), "for_each + prompt must be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("for_each"));
    }
}

mod executor {
    use ail_core::config::domain::{ContextSource, Step, StepBody, StepId};
    use ail_core::error::error_types;
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::test_helpers::make_session;

    /// s26.1 -- valid JSON response passes output_schema validation.
    #[test]
    fn valid_json_passes_schema_validation() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": { "category": { "type": "string" } },
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
        assert!(execute(&mut session, &runner).is_ok());
    }

    /// s26.1 -- non-JSON response fails output_schema validation.
    #[test]
    fn non_json_response_fails_validation() {
        let step = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            output_schema: Some(serde_json::json!({"type": "object"})),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("not json at all");
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// s26.1 -- JSON that doesn't match schema fails validation.
    #[test]
    fn json_not_matching_schema_fails_validation() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "priority": { "type": "integer" } },
            "required": ["priority"]
        });
        let step = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            output_schema: Some(schema),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new(r#"{"category": "bugfix"}"#);
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// s26.1 -- step without output_schema skips validation.
    #[test]
    fn step_without_schema_skips_validation() {
        let step = Step {
            id: StepId("plain".to_string()),
            body: StepBody::Prompt("hello".to_string()),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("just plain text, not JSON");
        assert!(execute(&mut session, &runner).is_ok());
    }

    /// s26.1 -- output_schema validation works on context:shell: steps.
    #[test]
    fn schema_validation_on_context_step() {
        let step = Step {
            id: StepId("cmd".to_string()),
            body: StepBody::Context(ContextSource::Shell(
                r#"echo '{"status": "ok"}'"#.to_string(),
            )),
            output_schema: Some(serde_json::json!({"type": "object", "required": ["status"]})),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// s26.2 -- valid preceding output passes input_schema validation.
    #[test]
    fn valid_input_passes_input_schema_validation() {
        let producer = Step {
            id: StepId("classify".to_string()),
            body: StepBody::Prompt("classify".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("handle".to_string()),
            body: StepBody::Prompt("handle".to_string()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": { "category": { "type": "string" } },
                "required": ["category"]
            })),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        let runner = StubRunner::new(r#"{"category": "bugfix"}"#);
        assert!(execute(&mut session, &runner).is_ok());
    }

    /// s26.2 -- non-JSON preceding output fails input_schema validation.
    #[test]
    fn non_json_input_fails_input_schema_validation() {
        let producer = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("handle".to_string()),
            body: StepBody::Prompt("handle".to_string()),
            input_schema: Some(serde_json::json!({"type": "object"})),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        let runner = StubRunner::new("not json");
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::INPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// s26.2 -- preceding output that doesn't match input_schema fails.
    #[test]
    fn mismatched_input_fails_input_schema_validation() {
        let producer = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("handle".to_string()),
            body: StepBody::Prompt("handle".to_string()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": { "priority": { "type": "integer" } },
                "required": ["priority"]
            })),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        // Producer outputs object without "priority"
        let runner = StubRunner::new(r#"{"name": "test"}"#);
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::INPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// s26.4 -- field:equals: matches and routes to if_true action.
    #[test]
    fn field_equals_matches_if_true() {
        use ail_core::config::domain::{ResultAction, ResultBranch, ResultMatcher};

        let producer = Step {
            id: StepId("classify".to_string()),
            body: StepBody::Prompt("classify".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("handle".to_string()),
            body: StepBody::Prompt("handle".to_string()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": { "category": { "type": "string" } },
                "required": ["category"]
            })),
            on_result: Some(vec![
                ResultBranch {
                    matcher: ResultMatcher::Field {
                        name: "category".to_string(),
                        equals: serde_json::json!("bugfix"),
                    },
                    action: ResultAction::Continue,
                },
                ResultBranch {
                    matcher: ResultMatcher::Always,
                    action: ResultAction::AbortPipeline,
                },
            ]),
            ..Default::default()
        };
        let third = Step {
            id: StepId("after".to_string()),
            body: StepBody::Prompt("after".to_string()),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer, third]);
        // Producer returns JSON with category=bugfix -> field matcher matches -> continue
        let runner = StubRunner::new(r#"{"category": "bugfix"}"#);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "Should continue past field:equals: match: {result:?}"
        );
        // The "after" step should have run
        assert!(
            session
                .turn_log
                .entries()
                .iter()
                .any(|e| e.step_id == "after"),
            "Third step should have executed"
        );
    }

    /// s26.4 -- field:equals: non-match routes to if_false (Always) action.
    #[test]
    fn field_equals_non_match_routes_to_if_false() {
        use ail_core::config::domain::{ResultAction, ResultBranch, ResultMatcher};

        let producer = Step {
            id: StepId("classify".to_string()),
            body: StepBody::Prompt("classify".to_string()),
            ..Default::default()
        };
        let consumer = Step {
            id: StepId("handle".to_string()),
            body: StepBody::Prompt("handle".to_string()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": { "category": { "type": "string" } },
                "required": ["category"]
            })),
            on_result: Some(vec![
                ResultBranch {
                    matcher: ResultMatcher::Field {
                        name: "category".to_string(),
                        equals: serde_json::json!("bugfix"),
                    },
                    action: ResultAction::Continue,
                },
                ResultBranch {
                    matcher: ResultMatcher::Always,
                    action: ResultAction::AbortPipeline,
                },
            ]),
            ..Default::default()
        };
        let mut session = make_session(vec![producer, consumer]);
        // Producer returns "feature" not "bugfix" -> field matcher doesn't match -> Always fires -> abort
        let runner = StubRunner::new(r#"{"category": "feature"}"#);
        let result = execute(&mut session, &runner);
        assert!(result.is_err(), "Should abort on non-match");
    }
}

mod template {
    use ail_core::config::domain::{Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::test_helpers::make_session;

    /// s26.5 -- {{ step.<id>.items }} resolves for array responses.
    #[test]
    fn items_template_variable_resolves_array() {
        let producer = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan tasks".to_string()),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": { "type": "string" }
            })),
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

    /// s26.5 -- {{ step.<id>.items }} fails for non-array responses.
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
        assert!(execute(&mut session, &runner).is_err());
    }

    /// s26.5 -- {{ step.<id>.items }} fails for non-JSON responses.
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
        assert!(execute(&mut session, &runner).is_err());
    }
}

mod invoke_options_propagation {
    use ail_core::config::domain::{Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::RecordingStubRunner;
    use ail_core::test_helpers::make_session;

    /// s26.7 -- output_schema is propagated into InvokeOptions so runners can
    /// pass it to providers for constrained decoding.
    #[test]
    fn output_schema_propagated_to_invoke_options() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "classification": {
                    "type": "string",
                    "enum": ["TRIVIAL", "EXPLICIT", "EXPLORATORY", "AMBIGUOUS"]
                }
            },
            "required": ["classification"]
        });
        let step = Step {
            id: StepId("classify".to_string()),
            body: StepBody::Prompt("classify".to_string()),
            output_schema: Some(schema.clone()),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = RecordingStubRunner::new(r#"{"classification": "TRIVIAL"}"#);
        execute(&mut session, &runner).expect("execution should succeed");

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].output_schema.as_ref(),
            Some(&schema),
            "output_schema must be forwarded into InvokeOptions"
        );
    }

    /// s26.7 -- steps without output_schema have None in InvokeOptions.
    #[test]
    fn no_output_schema_means_none_in_invoke_options() {
        let step = Step {
            id: StepId("plain".to_string()),
            body: StepBody::Prompt("hello".to_string()),
            ..Default::default()
        };
        let mut session = make_session(vec![step]);
        let runner = RecordingStubRunner::new("hello back");
        execute(&mut session, &runner).expect("execution should succeed");

        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].output_schema.is_none(),
            "steps without output_schema should pass None"
        );
    }
}
