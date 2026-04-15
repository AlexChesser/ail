//! SPEC §27 — do_while: bounded repeat-until loop validation tests.

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

mod executor {
    use ail_core::config::domain::{
        ConditionExpr, ConditionOp, ContextSource, ExitCodeMatch, ResultAction, ResultBranch,
        ResultMatcher, Step, StepBody, StepId,
    };
    use ail_core::error::error_types;
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::test_helpers::make_session;

    fn shell_step(id: &str, cmd: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Context(ContextSource::Shell(cmd.to_string())),
            ..Default::default()
        }
    }

    fn do_while_step(
        id: &str,
        max_iterations: u64,
        exit_when: ConditionExpr,
        inner_steps: Vec<Step>,
    ) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::DoWhile {
                max_iterations,
                exit_when,
                steps: inner_steps,
            },
            ..Default::default()
        }
    }

    fn exit_code_eq_zero(step_id: &str) -> ConditionExpr {
        ConditionExpr {
            lhs: format!("{{{{ step.{step_id}.exit_code }}}}"),
            op: ConditionOp::Eq,
            rhs: "0".to_string(),
        }
    }

    /// §27 — do_while loop exits on first iteration when exit_when is immediately true.
    #[test]
    fn do_while_exits_on_first_iteration() {
        // `true` exits with code 0, so exit_when is true after the first iteration.
        let step = do_while_step(
            "loop",
            5,
            exit_code_eq_zero("check"),
            vec![shell_step("check", "true")],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);
        assert!(result.is_ok(), "Expected Ok, got: {result:?}");

        // The loop step itself should have a TurnEntry, plus the inner step entry.
        let entries = session.turn_log.entries();
        assert!(entries.iter().any(|e| e.step_id == "loop"));
        assert!(entries.iter().any(|e| e.step_id == "loop::check"));
    }

    /// §27 — do_while loop runs multiple iterations when exit_when is false.
    #[test]
    fn do_while_runs_multiple_iterations() {
        // `false` exits with code 1, so exit_when is never true → hits max_iterations.
        let step = do_while_step(
            "retry",
            3,
            exit_code_eq_zero("check"),
            vec![shell_step("check", "false")],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::DO_WHILE_MAX_ITERATIONS);
        assert!(
            err.detail().contains("retry"),
            "Error should mention step id, got: {}",
            err.detail()
        );
    }

    /// §27 — inner step entries use namespaced IDs (loop_id::step_id).
    #[test]
    fn inner_step_entries_are_namespaced() {
        let step = do_while_step(
            "myloop",
            5,
            exit_code_eq_zero("check"),
            vec![shell_step("check", "true")],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        execute(&mut session, &runner).unwrap();

        let entries = session.turn_log.entries();
        let inner_entry = entries
            .iter()
            .find(|e| e.step_id == "myloop::check")
            .expect("Should have namespaced entry 'myloop::check'");
        assert_eq!(inner_entry.exit_code, Some(0));
    }

    /// §27 — exit_when with `contains` operator works.
    #[test]
    fn exit_when_contains_operator() {
        let exit_when = ConditionExpr {
            lhs: "{{ step.check.stdout }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "PASS".to_string(),
        };
        let step = do_while_step("loop", 5, exit_when, vec![shell_step("check", "echo PASS")]);
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "Expected loop to exit via contains, got: {result:?}"
        );
    }

    /// §27 — template variable `{{ do_while.iteration }}` resolves inside the loop.
    #[test]
    fn do_while_iteration_template_variable() {
        // Use a prompt step inside the loop so we can check the resolved prompt.
        let inner_prompt = Step {
            id: StepId("inner".to_string()),
            body: StepBody::Prompt(
                "Iteration {{ do_while.iteration }} of {{ do_while.max_iterations }}".to_string(),
            ),
            ..Default::default()
        };
        // exit_when on a prompt step: check the response contains "done"
        let exit_when = ConditionExpr {
            lhs: "{{ step.inner.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "stub".to_string(),
        };
        let step = do_while_step("loop", 3, exit_when, vec![inner_prompt]);
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub response");
        execute(&mut session, &runner).unwrap();

        // Check that the resolved prompt included the iteration variables.
        let inner_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop::inner")
            .expect("Should have 'loop::inner' entry");
        assert!(
            inner_entry.prompt.contains("Iteration 0 of 3"),
            "Expected 'Iteration 0 of 3' in prompt, got: {}",
            inner_entry.prompt
        );
    }

    /// §27 — only current iteration's entries are in scope (previous cleared).
    #[test]
    fn iteration_scope_clears_previous_entries() {
        // `false` always fails, so the loop runs 2 iterations and hits max.
        let step = do_while_step(
            "loop",
            2,
            exit_code_eq_zero("check"),
            vec![shell_step("check", "false")],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let _ = execute(&mut session, &runner); // max_iterations error expected

        // After the loop, only one set of namespaced entries should remain
        // (from the last iteration). There should NOT be two 'loop::check' entries.
        let check_entries: Vec<_> = session
            .turn_log
            .entries()
            .iter()
            .filter(|e| e.step_id == "loop::check")
            .collect();
        assert_eq!(
            check_entries.len(),
            1,
            "Expected 1 'loop::check' entry (last iteration only), got {}",
            check_entries.len()
        );
    }

    /// §27 — multiple inner steps execute in order each iteration.
    #[test]
    fn multiple_inner_steps_execute_in_order() {
        let inner_steps = vec![
            shell_step("step_a", "echo first"),
            shell_step("step_b", "true"), // exit code 0
        ];
        let step = do_while_step("loop", 3, exit_code_eq_zero("step_b"), inner_steps);
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        execute(&mut session, &runner).unwrap();

        let entries = session.turn_log.entries();
        let a_entry = entries
            .iter()
            .find(|e| e.step_id == "loop::step_a")
            .expect("step_a should be present");
        assert!(a_entry.stdout.as_deref().unwrap().contains("first"));

        let b_entry = entries
            .iter()
            .find(|e| e.step_id == "loop::step_b")
            .expect("step_b should be present");
        assert_eq!(b_entry.exit_code, Some(0));
    }

    /// §27 — `break` in on_result exits the loop, not the pipeline.
    #[test]
    fn break_exits_loop_not_pipeline() {
        // Shell step always exits 1, on_result triggers break.
        let inner = Step {
            id: StepId("check".to_string()),
            body: StepBody::Context(ContextSource::Shell("false".to_string())),
            on_result: Some(vec![ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
                action: ResultAction::Break,
            }]),
            ..Default::default()
        };
        // exit_when will never be true, but break should exit the loop.
        let step = do_while_step("loop", 5, exit_code_eq_zero("check"), vec![inner]);

        // Add a step AFTER the loop to verify pipeline continues.
        let after_step = Step {
            id: StepId("after".to_string()),
            body: StepBody::Context(ContextSource::Shell("echo after_loop".to_string())),
            ..Default::default()
        };

        let mut session = make_session(vec![step, after_step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);
        assert!(result.is_ok(), "Pipeline should continue after loop break");

        // Verify the step after the loop ran.
        let after_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "after")
            .expect("'after' step should have run");
        assert!(after_entry
            .stdout
            .as_deref()
            .unwrap()
            .contains("after_loop"));
    }

    /// §27 — loop depth limit is enforced.
    #[test]
    fn loop_depth_limit_enforced() {
        // Create a deeply nested do_while — 9 levels deep, exceeding MAX_LOOP_DEPTH (8).
        fn nested_do_while(remaining: usize) -> Step {
            if remaining == 0 {
                shell_step("leaf", "true")
            } else {
                let inner = nested_do_while(remaining - 1);
                do_while_step(
                    &format!("level_{remaining}"),
                    1,
                    exit_code_eq_zero("leaf"),
                    vec![inner],
                )
            }
        }

        let step = nested_do_while(9); // 9 levels > MAX_LOOP_DEPTH (8)
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::LOOP_DEPTH_EXCEEDED);
    }

    /// §27 — do_while summary entry has the loop step's response.
    #[test]
    fn summary_entry_has_loop_response() {
        // Prompt step inside loop: StubRunner returns "stub response".
        let inner = Step {
            id: StepId("inner".to_string()),
            body: StepBody::Prompt("test".to_string()),
            ..Default::default()
        };
        let exit_when = ConditionExpr {
            lhs: "{{ step.inner.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "stub".to_string(),
        };
        let step = do_while_step("loop", 3, exit_when, vec![inner]);
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub response");
        execute(&mut session, &runner).unwrap();

        let loop_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop")
            .expect("loop summary entry should exist");
        assert_eq!(
            loop_entry.response.as_deref(),
            Some("stub response"),
            "Loop summary entry should contain the inner step's response"
        );
    }

    /// §27 — summary entry has index count.
    #[test]
    fn summary_entry_has_index() {
        let inner = Step {
            id: StepId("inner".to_string()),
            body: StepBody::Prompt("test".to_string()),
            ..Default::default()
        };
        let exit_when = ConditionExpr {
            lhs: "{{ step.inner.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "stub".to_string(),
        };
        let step = do_while_step("loop", 5, exit_when, vec![inner]);
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub response");
        execute(&mut session, &runner).unwrap();

        let loop_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop")
            .expect("loop summary entry should exist");
        assert_eq!(
            loop_entry.index,
            Some(1),
            "Loop that exits after 1 iteration should have index=1"
        );
    }

    /// §27 — index is accessible via template variable.
    #[test]
    fn index_template_variable() {
        let inner = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            ..Default::default()
        };
        let exit_when = ConditionExpr {
            lhs: "{{ step.gen.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "stub".to_string(),
        };
        let loop_step = do_while_step("myloop", 5, exit_when, vec![inner]);

        let after = Step {
            id: StepId("report".to_string()),
            body: StepBody::Prompt("Loop took {{ step.myloop.index }} iterations".to_string()),
            ..Default::default()
        };

        let mut session = make_session(vec![loop_step, after]);
        let runner = StubRunner::new("stub response");
        execute(&mut session, &runner).unwrap();

        let report_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "report")
            .expect("'report' step should exist");
        assert!(
            report_entry.prompt.contains("Loop took 1 iterations"),
            "Expected index in prompt, got: {}",
            report_entry.prompt
        );
    }

    /// §27 — do_while with max_iterations: 1 either exits or errors.
    #[test]
    fn max_iterations_one_exits_or_errors() {
        // exit_when is true → exits cleanly.
        let step = do_while_step(
            "loop",
            1,
            exit_code_eq_zero("check"),
            vec![shell_step("check", "true")],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        assert!(execute(&mut session, &runner).is_ok());

        // exit_when is false → max_iterations exceeded.
        let step2 = do_while_step(
            "loop",
            1,
            exit_code_eq_zero("check"),
            vec![shell_step("check", "false")],
        );
        let mut session2 = make_session(vec![step2]);
        let err = execute(&mut session2, &runner).unwrap_err();
        assert_eq!(err.error_type(), error_types::DO_WHILE_MAX_ITERATIONS);
    }

    /// §27 — steps after the loop can reference the loop's summary entry.
    #[test]
    fn post_loop_step_references_loop_response() {
        let inner = Step {
            id: StepId("gen".to_string()),
            body: StepBody::Prompt("generate".to_string()),
            ..Default::default()
        };
        let exit_when = ConditionExpr {
            lhs: "{{ step.gen.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "stub".to_string(),
        };
        let loop_step = do_while_step("myloop", 3, exit_when, vec![inner]);

        // Step after the loop references the loop's response.
        let after = Step {
            id: StepId("use_result".to_string()),
            body: StepBody::Prompt("Loop said: {{ step.myloop.response }}".to_string()),
            ..Default::default()
        };

        let mut session = make_session(vec![loop_step, after]);
        let runner = StubRunner::new("stub response");
        execute(&mut session, &runner).unwrap();

        let after_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "use_result")
            .expect("'use_result' step should exist");
        assert!(
            after_entry.prompt.contains("Loop said: stub response"),
            "Expected resolved template, got: {}",
            after_entry.prompt
        );
    }

    /// §27 — qualified step reference (loop_id::step_id) works from outside the loop.
    #[test]
    fn qualified_step_reference_from_outside_loop() {
        let inner = shell_step("check", "echo hello_from_loop");
        let exit_when = ConditionExpr {
            lhs: "{{ step.check.exit_code }}".to_string(),
            op: ConditionOp::Eq,
            rhs: "0".to_string(),
        };
        let loop_step = do_while_step("myloop", 3, exit_when, vec![inner]);

        // After the loop, reference inner step using qualified form.
        let after = Step {
            id: StepId("after".to_string()),
            body: StepBody::Prompt("Inner said: {{ step.myloop::check.stdout }}".to_string()),
            ..Default::default()
        };

        let mut session = make_session(vec![loop_step, after]);
        let runner = StubRunner::new("stub");
        execute(&mut session, &runner).unwrap();

        let after_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "after")
            .expect("'after' step should exist");
        assert!(
            after_entry.prompt.contains("Inner said: hello_from_loop"),
            "Expected qualified reference to resolve, got: {}",
            after_entry.prompt
        );
    }

    /// §27 — abort_pipeline inside loop propagates to pipeline level.
    #[test]
    fn abort_pipeline_inside_loop_propagates() {
        let inner = Step {
            id: StepId("check".to_string()),
            body: StepBody::Context(ContextSource::Shell("false".to_string())),
            on_result: Some(vec![ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
                action: ResultAction::AbortPipeline,
            }]),
            ..Default::default()
        };
        let step = do_while_step("loop", 5, exit_code_eq_zero("check"), vec![inner]);
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type(), error_types::PIPELINE_ABORTED);
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
