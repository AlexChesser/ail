/// SPEC §28 — for_each collection iteration.

// ── Parse-time validation (valid configs) ───────────────────────────────────

mod parse_valid {
    use ail_core::config;
    use ail_core::config::domain::{OnMaxItems, StepBody};

    /// §28.1 — minimal valid for_each with defaults.
    #[test]
    fn minimal_for_each_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      steps:
        - id: work
          prompt: "do {{ for_each.item }}"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).expect("should parse");
        let step = &pipeline.steps[0];
        match &step.body {
            StepBody::ForEach {
                over,
                as_name,
                max_items,
                on_max_items,
                steps,
            } => {
                assert_eq!(over, "{{ step.plan.items }}");
                assert_eq!(as_name, "item"); // default
                assert_eq!(*max_items, None);
                assert_eq!(*on_max_items, OnMaxItems::Continue); // default
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].id.as_str(), "work");
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }

    /// §28.2 — for_each with all optional fields set.
    #[test]
    fn for_each_with_all_options() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      as: task
      max_items: 10
      on_max_items: abort_pipeline
      steps:
        - id: impl
          prompt: "implement {{ for_each.task }}"
        - id: verify
          prompt: "verify {{ for_each.task }}"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).expect("should parse");
        let step = &pipeline.steps[0];
        match &step.body {
            StepBody::ForEach {
                as_name,
                max_items,
                on_max_items,
                steps,
                ..
            } => {
                assert_eq!(as_name, "task");
                assert_eq!(*max_items, Some(10));
                assert_eq!(*on_max_items, OnMaxItems::AbortPipeline);
                assert_eq!(steps.len(), 2);
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }

    /// §28.2 — on_max_items: continue is the explicit form of the default.
    #[test]
    fn on_max_items_continue_explicit() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      max_items: 5
      on_max_items: continue
      steps:
        - id: work
          prompt: "do it"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).expect("should parse");
        match &pipeline.steps[0].body {
            StepBody::ForEach { on_max_items, .. } => {
                assert_eq!(*on_max_items, OnMaxItems::Continue);
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }
}

// ── Parse-time validation (invalid configs) ─────────────────────────────────

mod parse_invalid {
    use ail_core::config;
    use ail_core::error::error_types;

    /// §28.7 rule 1 — missing `over` is an error.
    #[test]
    fn missing_over() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      steps:
        - id: work
          prompt: "do it"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("over"), "got: {}", err.detail());
    }

    /// §28.7 rule 1 — missing `steps` is an error.
    #[test]
    fn missing_steps() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("steps"), "got: {}", err.detail());
    }

    /// §28.7 rule 1 — empty `steps` array is an error.
    #[test]
    fn empty_steps() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      steps: []
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("empty"),
            "should mention empty, got: {}",
            err.detail()
        );
    }

    /// §28.7 rule 3 — for_each is mutually exclusive with prompt.
    #[test]
    fn mutually_exclusive_with_prompt() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    prompt: "hello"
    for_each:
      over: "{{ step.plan.items }}"
      steps:
        - id: work
          prompt: "do it"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("primary field"),
            "got: {}",
            err.detail()
        );
    }

    /// §28.7 rule 4 — invalid `as` identifier (starts with digit).
    #[test]
    fn invalid_as_identifier() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      as: "1bad"
      steps:
        - id: work
          prompt: "do it"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("identifier"), "got: {}", err.detail());
    }

    /// §28.7 rule 5 — max_items: 0 is an error.
    #[test]
    fn max_items_zero() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      max_items: 0
      steps:
        - id: work
          prompt: "do it"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("max_items"), "got: {}", err.detail());
    }

    /// §28.7 rule 6 — unknown on_max_items value is an error.
    #[test]
    fn unknown_on_max_items() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      on_max_items: "skip"
      steps:
        - id: work
          prompt: "do it"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("on_max_items"),
            "got: {}",
            err.detail()
        );
    }

    /// §28.7 rule 7 — duplicate inner step IDs are rejected.
    #[test]
    fn duplicate_inner_step_ids() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      steps:
        - id: work
          prompt: "first"
        - id: work
          prompt: "duplicate"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("work"),
            "should mention duplicate id, got: {}",
            err.detail()
        );
    }
}

// ── Executor tests ──────────────────────────────────────────────────────────

mod executor {
    use ail_core::config::domain::{OnMaxItems, Step, StepBody, StepId};
    use ail_core::error::error_types;
    use ail_core::test_helpers::{make_session, prompt_step};

    fn for_each_step(id: &str, over: &str, as_name: &str, inner: Vec<Step>) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::ForEach {
                over: over.to_string(),
                as_name: as_name.to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: inner,
            },
            ..Default::default()
        }
    }

    /// §28.3 — for_each iterates over all items in the array.
    #[test]
    fn iterates_over_all_items() {
        // Set up: a prior step whose response is a JSON array.
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "task",
            vec![prompt_step("work", "do {{ for_each.task }}")],
        );
        let array_runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["alpha", "beta", "gamma"]"#.to_string(),
            "done-alpha".to_string(),
            "done-beta".to_string(),
            "done-gamma".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = ail_core::executor::execute(&mut session, &array_runner);
        assert!(result.is_ok(), "execution should succeed: {result:?}");

        // The plan step should have produced a JSON array response.
        let plan_resp = session.turn_log.response_for_step("plan").unwrap();
        assert_eq!(plan_resp, r#"["alpha", "beta", "gamma"]"#);

        // The loop step summary should be in the turn log.
        let loop_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop")
            .expect("loop step entry should exist");
        assert!(loop_entry.prompt.contains("for_each"));
    }

    /// §28.3 point 5 — break exits the loop, not the pipeline.
    #[test]
    fn break_exits_loop_not_pipeline() {
        use ail_core::config::domain::{ResultAction, ResultBranch, ResultMatcher};

        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let mut inner = prompt_step("work", "do it");
        inner.on_result = Some(vec![ResultBranch {
            matcher: ResultMatcher::Always,
            action: ResultAction::Break,
        }]);
        let fe = for_each_step("loop", "{{ step.plan.items }}", "item", vec![inner]);
        // Add a step after the loop to verify the pipeline continues.
        let after = prompt_step("after", "final");

        let array_runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["a", "b", "c"]"#.to_string(),
            "first-item-response".to_string(),
            "after-response".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe, after]);
        let result = ail_core::executor::execute(&mut session, &array_runner);
        assert!(result.is_ok(), "pipeline should complete: {result:?}");

        // The "after" step should have executed (break exits the loop, not the pipeline).
        assert!(
            session.turn_log.response_for_step("after").is_some(),
            "step after the loop should have executed"
        );
    }

    /// §28.2 — max_items caps the number of items processed.
    #[test]
    fn max_items_caps_processing() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = Step {
            id: StepId("loop".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "item".to_string(),
                max_items: Some(2),
                on_max_items: OnMaxItems::Continue,
                steps: vec![prompt_step("work", "do {{ for_each.item }}")],
            },
            ..Default::default()
        };

        let array_runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["a", "b", "c", "d"]"#.to_string(),
            "done-a".to_string(),
            "done-b".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = ail_core::executor::execute(&mut session, &array_runner);
        assert!(result.is_ok(), "execution should succeed: {result:?}");

        // Only 2 items should have been processed (runner called twice for inner steps).
        // The loop summary entry should reflect this.
        let loop_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop")
            .expect("loop entry");
        assert!(loop_entry.prompt.contains("items=2"));
    }

    /// §28.2 — on_max_items: abort_pipeline aborts when array exceeds max_items.
    #[test]
    fn on_max_items_abort_pipeline() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = Step {
            id: StepId("loop".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "item".to_string(),
                max_items: Some(2),
                on_max_items: OnMaxItems::AbortPipeline,
                steps: vec![prompt_step("work", "do it")],
            },
            ..Default::default()
        };

        let array_runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["a", "b", "c"]"#.to_string(), // 3 items, cap is 2
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = ail_core::executor::execute(&mut session, &array_runner);
        assert!(result.is_err(), "should abort");
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::PIPELINE_ABORTED);
        assert!(err.detail().contains("max_items"), "got: {}", err.detail());
    }

    /// §28.3 — for_each with a non-array source produces a template error
    /// (the .items accessor in template resolution rejects non-arrays).
    #[test]
    fn non_array_source_errors() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "item",
            vec![prompt_step("work", "do it")],
        );

        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"{"not": "an array"}"#.to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = ail_core::executor::execute(&mut session, &runner);
        assert!(result.is_err(), "should fail");
        let err = result.unwrap_err();
        // The .items template accessor catches non-arrays before for_each runs.
        assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
    }

    /// §28.3 — for_each with non-JSON source produces a template error
    /// (the .items accessor in template resolution rejects non-JSON).
    #[test]
    fn non_json_source_errors() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "item",
            vec![prompt_step("work", "do it")],
        );

        let runner =
            ail_core::runner::stub::SequenceStubRunner::new(vec!["not json at all".to_string()]);
        let mut session = make_session(vec![plan, fe]);
        let result = ail_core::executor::execute(&mut session, &runner);
        assert!(result.is_err(), "should fail");
        let err = result.unwrap_err();
        // The .items template accessor catches non-JSON before for_each runs.
        assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
    }

    /// §28.3 — empty array produces no iterations and completes cleanly.
    #[test]
    fn empty_array_completes() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "item",
            vec![prompt_step("work", "do it")],
        );

        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec!["[]".to_string()]);
        let mut session = make_session(vec![plan, fe]);
        let result = ail_core::executor::execute(&mut session, &runner);
        assert!(result.is_ok(), "empty array should succeed: {result:?}");
    }

    /// §28.3 point 2 — for_each step IDs are namespaced.
    #[test]
    fn inner_steps_are_namespaced() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "item",
            vec![prompt_step("work", "do it")],
        );

        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["x"]"#.to_string(),
            "done".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        ail_core::executor::execute(&mut session, &runner).unwrap();

        // The inner step should appear with a namespaced ID.
        let has_namespaced = session
            .turn_log
            .entries()
            .iter()
            .any(|e| e.step_id == "loop::work");
        assert!(
            has_namespaced,
            "inner step should be namespaced as loop::work"
        );
    }
}

// ── Template variable tests ─────────────────────────────────────────────────

mod template {
    use ail_core::config::domain::{OnMaxItems, Step, StepBody, StepId};
    use ail_core::test_helpers::{make_session, prompt_step};

    fn for_each_step(id: &str, over: &str, as_name: &str, inner: Vec<Step>) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::ForEach {
                over: over.to_string(),
                as_name: as_name.to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: inner,
            },
            ..Default::default()
        }
    }

    /// §28.4 — for_each.index is 1-based.
    #[test]
    fn for_each_index_is_one_based() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "item",
            vec![prompt_step("work", "index={{ for_each.index }}")],
        );

        // The inner step prompt will contain the resolved index.
        // SequenceStubRunner echoes prompts if we run out of items, but let's use
        // a runner that returns fixed responses. The prompt itself is what we care about.
        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["a", "b"]"#.to_string(),
            "resp1".to_string(),
            "resp2".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        ail_core::executor::execute(&mut session, &runner).unwrap();

        // The last inner step entry should have been for the second item (index=2).
        // Due to item scope (entries cleared between items), only the last item's
        // entries remain. Check the prompt of the remaining entry.
        let work_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop::work")
            .expect("should find work entry");
        assert!(
            work_entry.prompt.contains("index=2"),
            "last item should have index=2, got prompt: {}",
            work_entry.prompt
        );
    }

    /// §28.4 — for_each.total reflects the collection size.
    #[test]
    fn for_each_total() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "item",
            vec![prompt_step("work", "total={{ for_each.total }}")],
        );

        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["a", "b", "c"]"#.to_string(),
            "r1".to_string(),
            "r2".to_string(),
            "r3".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        ail_core::executor::execute(&mut session, &runner).unwrap();

        let work_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop::work")
            .expect("should find work entry");
        assert!(
            work_entry.prompt.contains("total=3"),
            "total should be 3, got prompt: {}",
            work_entry.prompt
        );
    }

    /// §28.4 — for_each.<as_name> resolves to the current item value.
    #[test]
    fn for_each_custom_as_name() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "task",
            vec![prompt_step("work", "do: {{ for_each.task }}")],
        );

        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["build it"]"#.to_string(),
            "done".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        ail_core::executor::execute(&mut session, &runner).unwrap();

        let work_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop::work")
            .expect("should find work entry");
        assert!(
            work_entry.prompt.contains("do: build it"),
            "item should resolve by as name, got prompt: {}",
            work_entry.prompt
        );
    }

    /// §28.4 — for_each.item always works (even when as: is set to something else).
    #[test]
    fn for_each_item_always_available() {
        let plan = prompt_step("plan", "{{ step.invocation.prompt }}");
        let fe = for_each_step(
            "loop",
            "{{ step.plan.items }}",
            "task",
            vec![prompt_step("work", "item={{ for_each.item }}")],
        );

        let runner = ail_core::runner::stub::SequenceStubRunner::new(vec![
            r#"["hello"]"#.to_string(),
            "done".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        ail_core::executor::execute(&mut session, &runner).unwrap();

        let work_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop::work")
            .expect("should find work entry");
        assert!(
            work_entry.prompt.contains("item=hello"),
            "for_each.item should always work, got prompt: {}",
            work_entry.prompt
        );
    }

    /// §28.4 — for_each.* variables are not available outside the loop.
    #[test]
    fn for_each_vars_unavailable_outside_loop() {
        let step = prompt_step("test", "{{ for_each.index }}");
        let mut session = make_session(vec![step]);
        let runner = ail_core::runner::stub::CountingStubRunner::new("unused");
        let result = ail_core::executor::execute(&mut session, &runner);
        assert!(result.is_err(), "should fail outside loop");
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            ail_core::error::error_types::TEMPLATE_UNRESOLVED
        );
    }
}

// ── Materialize tests ───────────────────────────────────────────────────────

mod materialize {
    use ail_core::config::domain::{OnMaxItems, Pipeline, ProviderConfig, Step, StepBody, StepId};
    use std::path::PathBuf;

    #[test]
    fn for_each_appears_in_materialize_output() {
        let pipeline = Pipeline {
            steps: vec![Step {
                id: StepId("loop".to_string()),
                body: StepBody::ForEach {
                    over: "{{ step.plan.items }}".to_string(),
                    as_name: "task".to_string(),
                    max_items: Some(10),
                    on_max_items: OnMaxItems::AbortPipeline,
                    steps: vec![Step {
                        id: StepId("work".to_string()),
                        body: StepBody::Prompt("do it".to_string()),
                        ..Default::default()
                    }],
                },
                ..Default::default()
            }],
            source: Some(PathBuf::from("test.ail.yaml")),
            defaults: ProviderConfig::default(),
            timeout_seconds: None,
            default_tools: None,
            named_pipelines: Default::default(),
        };
        let output = ail_core::materialize::materialize(&pipeline);
        assert!(output.contains("for_each:"), "for_each: key missing");
        assert!(
            output.contains("over:"),
            "over: key missing in output: {output}"
        );
        assert!(
            output.contains("step.plan.items"),
            "over value missing in output: {output}"
        );
        assert!(output.contains("as: task"), "as: key missing: {output}");
        assert!(
            output.contains("max_items: 10"),
            "max_items missing: {output}"
        );
        assert!(
            output.contains("abort_pipeline"),
            "on_max_items missing: {output}"
        );
        assert!(output.contains("steps:"), "steps: key missing: {output}");
        assert!(output.contains("work"), "inner step missing: {output}");

        // Should be valid YAML.
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
        assert!(result.is_ok(), "Output was not valid YAML: {output}");
    }
}

// ── Pipeline-as-file tests ──────────────────────────────────────────────────

mod pipeline_as_file {
    use ail_core::config;
    use ail_core::config::domain::StepBody;
    use ail_core::error::error_types;

    /// §28.2 — for_each accepts pipeline: as alternative to inline steps.
    #[test]
    fn for_each_pipeline_file_loads_steps() {
        // Create the sub-pipeline file.
        let sub_dir = tempfile::tempdir().unwrap();
        let sub_path = sub_dir.path().join("loop-body.ail.yaml");
        std::fs::write(
            &sub_path,
            r#"
version: "0.1"
pipeline:
  - id: inner_work
    prompt: "do the work"
"#,
        )
        .unwrap();

        // Create the main pipeline that references it.
        let main_yaml = format!(
            r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{{{ step.plan.items }}}}"
      pipeline: {}
"#,
            sub_path.display()
        );
        let main_path = sub_dir.path().join("main.ail.yaml");
        std::fs::write(&main_path, main_yaml).unwrap();

        let pipeline = config::load(&main_path).expect("should parse");
        let step = &pipeline.steps[0];
        match &step.body {
            StepBody::ForEach { steps, .. } => {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].id.as_str(), "inner_work");
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }

    /// §28.2 — for_each with relative pipeline path resolves against source dir.
    #[test]
    fn for_each_pipeline_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        // Create sub-pipeline in a subdirectory.
        let sub_dir = dir.path().join("subs");
        std::fs::create_dir(&sub_dir).unwrap();
        std::fs::write(
            sub_dir.join("body.ail.yaml"),
            r#"
version: "0.1"
pipeline:
  - id: task
    prompt: "handle it"
"#,
        )
        .unwrap();

        // Main pipeline uses relative path.
        std::fs::write(
            dir.path().join("main.ail.yaml"),
            r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      pipeline: ./subs/body.ail.yaml
"#,
        )
        .unwrap();

        let pipeline =
            config::load(&dir.path().join("main.ail.yaml")).expect("should resolve relative path");
        match &pipeline.steps[0].body {
            StepBody::ForEach { steps, .. } => {
                assert_eq!(steps[0].id.as_str(), "task");
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }

    /// §28.7 rule 1 — declaring both steps and pipeline is an error.
    #[test]
    fn for_each_steps_and_pipeline_mutually_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("body.ail.yaml"),
            "version: \"0.1\"\npipeline:\n  - id: x\n    prompt: hi\n",
        )
        .unwrap();
        let yaml = format!(
            r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{{{ step.plan.items }}}}"
      pipeline: {}
      steps:
        - id: work
          prompt: "do it"
"#,
            dir.path().join("body.ail.yaml").display()
        );
        let main_path = dir.path().join("main.ail.yaml");
        std::fs::write(&main_path, yaml).unwrap();
        let err = config::load(&main_path).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("mutually exclusive"),
            "got: {}",
            err.detail()
        );
    }

    /// §28.7 rule 1 — declaring neither steps nor pipeline is an error.
    #[test]
    fn for_each_neither_steps_nor_pipeline() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("neither"), "got: {}", err.detail());
    }

    /// §28.7 rule 8 — missing pipeline file is a CONFIG_FILE_NOT_FOUND error.
    #[test]
    fn for_each_pipeline_file_not_found() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: loop
    for_each:
      over: "{{ step.plan.items }}"
      pipeline: ./nonexistent.ail.yaml
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let err = config::load(tmp.path()).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_FILE_NOT_FOUND);
    }

    /// §27.2 — do_while also accepts pipeline: as alternative to inline steps.
    #[test]
    fn do_while_pipeline_file_loads_steps() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("loop-body.ail.yaml"),
            r#"
version: "0.1"
pipeline:
  - id: fix
    prompt: "fix the code"
  - id: test
    context:
      shell: "echo ok"
"#,
        )
        .unwrap();

        let main_yaml = format!(
            r#"
version: "0.1"
pipeline:
  - id: fix_loop
    do_while:
      max_iterations: 5
      exit_when: "{{{{ step.test.exit_code }}}} == 0"
      pipeline: {}
"#,
            dir.path().join("loop-body.ail.yaml").display()
        );
        let main_path = dir.path().join("main.ail.yaml");
        std::fs::write(&main_path, main_yaml).unwrap();

        let pipeline = config::load(&main_path).expect("should parse");
        match &pipeline.steps[0].body {
            StepBody::DoWhile { steps, .. } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].id.as_str(), "fix");
                assert_eq!(steps[1].id.as_str(), "test");
            }
            other => panic!("expected DoWhile, got {other:?}"),
        }
    }

    /// §27.2 — do_while: declaring both steps and pipeline is an error.
    #[test]
    fn do_while_steps_and_pipeline_mutually_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("body.ail.yaml"),
            "version: \"0.1\"\npipeline:\n  - id: x\n    prompt: hi\n",
        )
        .unwrap();
        let yaml = format!(
            r#"
version: "0.1"
pipeline:
  - id: fix_loop
    do_while:
      max_iterations: 3
      exit_when: "{{{{ step.test.exit_code }}}} == 0"
      pipeline: {}
      steps:
        - id: test
          context:
            shell: "echo ok"
"#,
            dir.path().join("body.ail.yaml").display()
        );
        let main_path = dir.path().join("main.ail.yaml");
        std::fs::write(&main_path, yaml).unwrap();
        let err = config::load(&main_path).unwrap_err();
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(
            err.detail().contains("mutually exclusive"),
            "got: {}",
            err.detail()
        );
    }
}
