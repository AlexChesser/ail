/// Cross-feature integration tests for §26 (output_schema/input_schema),
/// §27 (do_while), and §28 (for_each).
///
/// These tests verify that the three feature areas compose correctly when
/// used together in a single pipeline — the scenarios that individual
/// per-section test files cannot cover.

// ── §26 + §28: for_each consuming schema-validated arrays ──────────────────

mod schema_and_for_each {
    use ail_core::config::domain::{OnMaxItems, Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::SequenceStubRunner;
    use ail_core::test_helpers::make_session;

    /// The canonical plan→implement pattern: a step produces a schema-validated
    /// JSON array, and a for_each step iterates over it via {{ step.<id>.items }}.
    #[test]
    fn for_each_consumes_schema_validated_array() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("generate tasks".to_string()),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": { "type": "string" }
            })),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("impl_tasks".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "task".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("work".to_string()),
                    body: StepBody::Prompt("do {{ for_each.task }}".to_string()),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };
        let after = Step {
            id: StepId("summary".to_string()),
            body: StepBody::Prompt("summarize".to_string()),
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["auth","tests","docs"]"#.to_string(), // plan response
            "done-auth".to_string(),                  // work iteration 1
            "done-tests".to_string(),                 // work iteration 2
            "done-docs".to_string(),                  // work iteration 3
            "all done".to_string(),                   // summary
        ]);
        let mut session = make_session(vec![plan, fe, after]);
        let result = execute(&mut session, &runner);
        assert!(result.is_ok(), "pipeline should complete: {result:?}");

        // Plan step should have stored a valid JSON array response.
        let plan_resp = session.turn_log.response_for_step("plan").unwrap();
        assert!(plan_resp.starts_with('['));

        // The summary step should have executed (pipeline continues after for_each).
        assert!(
            session.turn_log.response_for_step("summary").is_some(),
            "step after for_each should execute"
        );
    }

    /// output_schema validation on an inner step inside for_each — each iteration
    /// independently validates its response against the schema.
    #[test]
    fn output_schema_on_for_each_inner_step() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("loop".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "item".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("gen".to_string()),
                    body: StepBody::Prompt("generate for {{ for_each.item }}".to_string()),
                    output_schema: Some(serde_json::json!({
                        "type": "object",
                        "properties": { "status": { "type": "string" } },
                        "required": ["status"]
                    })),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };

        // Plan returns a 2-item array; each inner step returns valid JSON.
        let runner = SequenceStubRunner::new(vec![
            r#"["a","b"]"#.to_string(),
            r#"{"status":"ok"}"#.to_string(),
            r#"{"status":"ok"}"#.to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "valid inner output should pass schema: {result:?}"
        );
    }

    /// output_schema validation failure on an inner step aborts the pipeline
    /// (default on_error behaviour).
    #[test]
    fn output_schema_failure_inside_for_each_aborts() {
        use ail_core::error::error_types;

        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("loop".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "item".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("gen".to_string()),
                    body: StepBody::Prompt("generate".to_string()),
                    output_schema: Some(serde_json::json!({
                        "type": "object",
                        "required": ["status"]
                    })),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };

        // Plan returns array; inner step returns invalid JSON (missing required field).
        let runner = SequenceStubRunner::new(vec![
            r#"["a"]"#.to_string(),
            r#"{"wrong_field": 1}"#.to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// for_each with max_items caps iteration count even when the schema-validated
    /// array has more elements.
    #[test]
    fn for_each_max_items_caps_schema_array() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": { "type": "string" }
            })),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("loop".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "item".to_string(),
                max_items: Some(2),
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("work".to_string()),
                    body: StepBody::Prompt("do {{ for_each.item }}".to_string()),
                    ..Default::default()
                }],
            },
            ..Default::default()
        };

        // Plan returns 4 items, but max_items is 2 — only first 2 should be processed.
        let runner = SequenceStubRunner::new(vec![
            r#"["a","b","c","d"]"#.to_string(),
            "done-a".to_string(),
            "done-b".to_string(),
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = execute(&mut session, &runner);
        assert!(result.is_ok(), "should cap at max_items: {result:?}");

        // The loop summary entry records the effective item count.
        // Note: prior items' inner entries are cleared (§28.3 point 4 — item scope),
        // so we check the summary entry which reflects how many items were processed.
        let loop_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "loop")
            .expect("loop summary entry should exist");
        assert!(
            loop_entry.prompt.contains("items=2"),
            "summary should reflect max_items cap, got: {}",
            loop_entry.prompt
        );
    }
}

// ── §26 + §27: do_while with schema validation ────────────────────────────

mod schema_and_do_while {
    use ail_core::config::domain::{
        ConditionExpr, ConditionOp, ContextSource, Step, StepBody, StepId,
    };
    use ail_core::error::error_types;
    use ail_core::executor::execute;
    use ail_core::runner::stub::StubRunner;
    use ail_core::test_helpers::make_session;

    /// output_schema validation on an inner step inside do_while — the schema
    /// is checked each iteration.
    #[test]
    fn output_schema_failure_inside_do_while_aborts() {
        let dw = Step {
            id: StepId("loop".to_string()),
            body: StepBody::DoWhile {
                max_iterations: 3,
                exit_when: ConditionExpr {
                    lhs: "{{ step.check.exit_code }}".to_string(),
                    op: ConditionOp::Eq,
                    rhs: "0".to_string(),
                },
                steps: vec![
                    Step {
                        id: StepId("gen".to_string()),
                        body: StepBody::Prompt("fix it".to_string()),
                        output_schema: Some(serde_json::json!({
                            "type": "object",
                            "required": ["status"]
                        })),
                        ..Default::default()
                    },
                    Step {
                        id: StepId("check".to_string()),
                        body: StepBody::Context(ContextSource::Shell("echo ok".to_string())),
                        ..Default::default()
                    },
                ],
            },
            ..Default::default()
        };

        // Runner returns invalid JSON on first iteration (missing required field).
        let runner = StubRunner::new(r#"{"wrong": 1}"#);
        let mut session = make_session(vec![dw]);
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::OUTPUT_SCHEMA_VALIDATION_FAILED
        );
    }

    /// input_schema validation inside do_while — the inner step validates its
    /// input (the preceding inner step's output) each iteration.
    #[test]
    fn input_schema_inside_do_while() {
        use ail_core::runner::stub::SequenceStubRunner;

        let dw = Step {
            id: StepId("loop".to_string()),
            body: StepBody::DoWhile {
                max_iterations: 2,
                exit_when: ConditionExpr {
                    lhs: "{{ step.check.exit_code }}".to_string(),
                    op: ConditionOp::Eq,
                    rhs: "0".to_string(),
                },
                steps: vec![
                    Step {
                        id: StepId("gen".to_string()),
                        body: StepBody::Prompt("generate".to_string()),
                        ..Default::default()
                    },
                    Step {
                        id: StepId("validate".to_string()),
                        body: StepBody::Prompt("validate".to_string()),
                        input_schema: Some(serde_json::json!({
                            "type": "object",
                            "properties": { "code": { "type": "string" } },
                            "required": ["code"]
                        })),
                        ..Default::default()
                    },
                    Step {
                        id: StepId("check".to_string()),
                        body: StepBody::Context(ContextSource::Shell("exit 0".to_string())),
                        ..Default::default()
                    },
                ],
            },
            ..Default::default()
        };

        // gen returns valid JSON, validate receives it, check exits 0 → loop exits.
        let runner = SequenceStubRunner::new(vec![
            r#"{"code":"fn main() {}"}"#.to_string(), // gen
            "looks good".to_string(),                 // validate
        ]);
        let mut session = make_session(vec![dw]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "valid input should pass schema inside do_while: {result:?}"
        );
    }
}

// ── §27 + §28: nested loops ───────────────────────────────────────────────

mod nested_loops {
    use ail_core::config::domain::{
        ConditionExpr, ConditionOp, ContextSource, OnMaxItems, Step, StepBody, StepId,
    };
    use ail_core::error::error_types;
    use ail_core::executor::execute;
    use ail_core::runner::stub::SequenceStubRunner;
    use ail_core::test_helpers::make_session;

    /// for_each inside do_while — the outer loop repeats, and each iteration
    /// contains a for_each that iterates over items.
    #[test]
    fn for_each_inside_do_while() {
        // Outer: do_while that runs until check exits 0.
        // Inner: for_each over items from a plan step.
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let dw = Step {
            id: StepId("outer".to_string()),
            body: StepBody::DoWhile {
                max_iterations: 2,
                exit_when: ConditionExpr {
                    lhs: "{{ step.check.exit_code }}".to_string(),
                    op: ConditionOp::Eq,
                    rhs: "0".to_string(),
                },
                steps: vec![
                    Step {
                        id: StepId("inner_loop".to_string()),
                        body: StepBody::ForEach {
                            over: "{{ step.plan.items }}".to_string(),
                            as_name: "task".to_string(),
                            max_items: None,
                            on_max_items: OnMaxItems::Continue,
                            steps: vec![Step {
                                id: StepId("work".to_string()),
                                body: StepBody::Prompt("do {{ for_each.task }}".to_string()),
                                ..Default::default()
                            }],
                        },
                        ..Default::default()
                    },
                    Step {
                        id: StepId("check".to_string()),
                        body: StepBody::Context(ContextSource::Shell("exit 0".to_string())),
                        ..Default::default()
                    },
                ],
            },
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["a","b"]"#.to_string(), // plan
            "done-a".to_string(),       // work (item 1, iteration 0)
            "done-b".to_string(),       // work (item 2, iteration 0)
                                        // check exits 0 → loop exits after iteration 0
        ]);
        let mut session = make_session(vec![plan, dw]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "for_each inside do_while should work: {result:?}"
        );
    }

    /// do_while inside for_each — each item triggers a retry loop.
    #[test]
    fn do_while_inside_for_each() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("items".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "task".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("retry".to_string()),
                    body: StepBody::DoWhile {
                        max_iterations: 2,
                        exit_when: ConditionExpr {
                            lhs: "{{ step.test.exit_code }}".to_string(),
                            op: ConditionOp::Eq,
                            rhs: "0".to_string(),
                        },
                        steps: vec![
                            Step {
                                id: StepId("fix".to_string()),
                                body: StepBody::Prompt("fix {{ for_each.task }}".to_string()),
                                ..Default::default()
                            },
                            Step {
                                id: StepId("test".to_string()),
                                body: StepBody::Context(ContextSource::Shell("exit 0".to_string())),
                                ..Default::default()
                            },
                        ],
                    },
                    ..Default::default()
                }],
            },
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["alpha","beta"]"#.to_string(), // plan
            "fixed-alpha".to_string(),         // fix (item 1, iter 0 → test exits 0)
            "fixed-beta".to_string(),          // fix (item 2, iter 0 → test exits 0)
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "do_while inside for_each should work: {result:?}"
        );
    }

    /// Triple nesting: do_while > for_each > do_while — verifies depth guard
    /// handles mixed loop types correctly (each loop increments shared depth).
    #[test]
    fn triple_nested_loops_within_depth_limit() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let outer = Step {
            id: StepId("outer".to_string()),
            body: StepBody::DoWhile {
                max_iterations: 1,
                exit_when: ConditionExpr {
                    lhs: "{{ step.outer_check.exit_code }}".to_string(),
                    op: ConditionOp::Eq,
                    rhs: "0".to_string(),
                },
                steps: vec![
                    Step {
                        id: StepId("mid".to_string()),
                        body: StepBody::ForEach {
                            over: "{{ step.plan.items }}".to_string(),
                            as_name: "item".to_string(),
                            max_items: None,
                            on_max_items: OnMaxItems::Continue,
                            steps: vec![Step {
                                id: StepId("inner".to_string()),
                                body: StepBody::DoWhile {
                                    max_iterations: 1,
                                    exit_when: ConditionExpr {
                                        lhs: "{{ step.inner_check.exit_code }}".to_string(),
                                        op: ConditionOp::Eq,
                                        rhs: "0".to_string(),
                                    },
                                    steps: vec![
                                        Step {
                                            id: StepId("work".to_string()),
                                            body: StepBody::Prompt("do".to_string()),
                                            ..Default::default()
                                        },
                                        Step {
                                            id: StepId("inner_check".to_string()),
                                            body: StepBody::Context(ContextSource::Shell(
                                                "exit 0".to_string(),
                                            )),
                                            ..Default::default()
                                        },
                                    ],
                                },
                                ..Default::default()
                            }],
                        },
                        ..Default::default()
                    },
                    Step {
                        id: StepId("outer_check".to_string()),
                        body: StepBody::Context(ContextSource::Shell("exit 0".to_string())),
                        ..Default::default()
                    },
                ],
            },
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["x"]"#.to_string(), // plan
            "done-x".to_string(),   // work
        ]);
        let mut session = make_session(vec![plan, outer]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "3-deep nesting should be within depth limit: {result:?}"
        );
    }

    /// Depth limit is enforced across mixed loop types. 9 nested loops should
    /// exceed MAX_LOOP_DEPTH (8).
    #[test]
    fn depth_limit_enforced_across_mixed_loops() {
        // Build 9 levels of alternating do_while/for_each nesting.
        // Each level wraps the next. The innermost is a prompt step.
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };

        // Start with innermost step.
        let innermost = Step {
            id: StepId("innermost".to_string()),
            body: StepBody::Prompt("deep".to_string()),
            ..Default::default()
        };

        // Wrap in 9 alternating loops.
        let mut current = innermost;
        for level in 0..9 {
            let id = format!("level_{level}");
            if level % 2 == 0 {
                current = Step {
                    id: StepId(id),
                    body: StepBody::ForEach {
                        over: "{{ step.plan.items }}".to_string(),
                        as_name: "item".to_string(),
                        max_items: None,
                        on_max_items: OnMaxItems::Continue,
                        steps: vec![current],
                    },
                    ..Default::default()
                };
            } else {
                current = Step {
                    id: StepId(id),
                    body: StepBody::DoWhile {
                        max_iterations: 1,
                        exit_when: ConditionExpr {
                            lhs: "{{ do_while.iteration }}".to_string(),
                            op: ConditionOp::Eq,
                            rhs: "0".to_string(),
                        },
                        steps: vec![current],
                    },
                    ..Default::default()
                };
            }
        }

        let runner = SequenceStubRunner::new(vec![r#"["x"]"#.to_string(), "deep".to_string()]);
        let mut session = make_session(vec![plan, current]);
        let err = execute(&mut session, &runner).unwrap_err();
        assert_eq!(err.error_type(), error_types::LOOP_DEPTH_EXCEEDED);
    }
}

// ── §26 + §28: field:equals: inside for_each ──────────────────────────────

mod field_equals_in_loops {
    use ail_core::config::domain::{
        OnMaxItems, ResultAction, ResultBranch, ResultMatcher, Step, StepBody, StepId,
    };
    use ail_core::executor::execute;
    use ail_core::runner::stub::SequenceStubRunner;
    use ail_core::test_helpers::make_session;

    /// field:equals: routing inside a for_each body — each iteration evaluates
    /// the field matcher independently.
    #[test]
    fn field_equals_routes_inside_for_each() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("loop".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "item".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![
                    // First inner step produces JSON with a "status" field.
                    Step {
                        id: StepId("classify".to_string()),
                        body: StepBody::Prompt("classify".to_string()),
                        ..Default::default()
                    },
                    // Second inner step uses field:equals: to route based on status.
                    Step {
                        id: StepId("handle".to_string()),
                        body: StepBody::Prompt("handle".to_string()),
                        input_schema: Some(serde_json::json!({
                            "type": "object",
                            "properties": { "status": { "type": "string" } },
                            "required": ["status"]
                        })),
                        on_result: Some(vec![
                            ResultBranch {
                                matcher: ResultMatcher::Field {
                                    name: "status".to_string(),
                                    equals: serde_json::json!("done"),
                                },
                                action: ResultAction::Break,
                            },
                            ResultBranch {
                                matcher: ResultMatcher::Always,
                                action: ResultAction::Continue,
                            },
                        ]),
                        ..Default::default()
                    },
                ],
            },
            ..Default::default()
        };
        let after = Step {
            id: StepId("after".to_string()),
            body: StepBody::Prompt("after loop".to_string()),
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["a","b","c"]"#.to_string(),     // plan
            r#"{"status":"done"}"#.to_string(), // classify (item 1)
            "handled".to_string(),              // handle (item 1) → field matches "done" → break
            "after-resp".to_string(),           // after
        ]);
        let mut session = make_session(vec![plan, fe, after]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "pipeline should complete after break: {result:?}"
        );

        // The "after" step should execute (break exits loop, not pipeline).
        assert!(
            session.turn_log.response_for_step("after").is_some(),
            "step after loop should execute after break"
        );
    }
}

// ── §26 + §27 + §28: full pipeline integration ────────────────────────────

mod full_pipeline {
    use ail_core::config::domain::{
        ConditionExpr, ConditionOp, ContextSource, OnMaxItems, Step, StepBody, StepId,
    };
    use ail_core::executor::execute;
    use ail_core::runner::stub::SequenceStubRunner;
    use ail_core::test_helpers::make_session;

    /// Full integration: plan (output_schema array) → for_each → inner do_while
    /// retry loop. This is the canonical "plan tasks, implement each with retries"
    /// pattern that motivates the entire §26/§27/§28 feature set.
    #[test]
    fn plan_for_each_do_while_integration() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("break into tasks".to_string()),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": { "type": "string" }
            })),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("tasks".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "task".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("retry_loop".to_string()),
                    body: StepBody::DoWhile {
                        max_iterations: 3,
                        exit_when: ConditionExpr {
                            lhs: "{{ step.test.exit_code }}".to_string(),
                            op: ConditionOp::Eq,
                            rhs: "0".to_string(),
                        },
                        steps: vec![
                            Step {
                                id: StepId("impl".to_string()),
                                body: StepBody::Prompt("implement {{ for_each.task }}".to_string()),
                                ..Default::default()
                            },
                            Step {
                                id: StepId("test".to_string()),
                                body: StepBody::Context(ContextSource::Shell("exit 0".to_string())),
                                ..Default::default()
                            },
                        ],
                    },
                    ..Default::default()
                }],
            },
            ..Default::default()
        };
        let summary = Step {
            id: StepId("summary".to_string()),
            body: StepBody::Prompt("summarize".to_string()),
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["auth","api"]"#.to_string(), // plan
            "impl-auth".to_string(),         // impl (task 1, iter 0) → test exits 0
            "impl-api".to_string(),          // impl (task 2, iter 0) → test exits 0
            "all done".to_string(),          // summary
        ]);
        let mut session = make_session(vec![plan, fe, summary]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "full plan→for_each→do_while pipeline should complete: {result:?}"
        );

        // Summary step should have run.
        assert!(
            session.turn_log.response_for_step("summary").is_some(),
            "summary step should execute after nested loops"
        );
    }

    /// Template variables from both loop contexts are accessible when loops
    /// are nested — {{ for_each.task }} is available inside a do_while that
    /// lives inside a for_each.
    #[test]
    fn nested_loop_template_vars_accessible() {
        let plan = Step {
            id: StepId("plan".to_string()),
            body: StepBody::Prompt("plan".to_string()),
            ..Default::default()
        };
        let fe = Step {
            id: StepId("tasks".to_string()),
            body: StepBody::ForEach {
                over: "{{ step.plan.items }}".to_string(),
                as_name: "task".to_string(),
                max_items: None,
                on_max_items: OnMaxItems::Continue,
                steps: vec![Step {
                    id: StepId("retry".to_string()),
                    body: StepBody::DoWhile {
                        max_iterations: 1,
                        exit_when: ConditionExpr {
                            lhs: "{{ step.check.exit_code }}".to_string(),
                            op: ConditionOp::Eq,
                            rhs: "0".to_string(),
                        },
                        steps: vec![
                            Step {
                                id: StepId("work".to_string()),
                                body: StepBody::Prompt(
                                    "task={{ for_each.task }} iter={{ do_while.iteration }}"
                                        .to_string(),
                                ),
                                ..Default::default()
                            },
                            Step {
                                id: StepId("check".to_string()),
                                body: StepBody::Context(ContextSource::Shell("exit 0".to_string())),
                                ..Default::default()
                            },
                        ],
                    },
                    ..Default::default()
                }],
            },
            ..Default::default()
        };

        let runner = SequenceStubRunner::new(vec![
            r#"["auth"]"#.to_string(), // plan
            "done".to_string(),        // work (task=auth, iter=0)
        ]);
        let mut session = make_session(vec![plan, fe]);
        let result = execute(&mut session, &runner);
        assert!(
            result.is_ok(),
            "nested template vars should resolve: {result:?}"
        );

        // Verify the prompt was resolved with both for_each and do_while vars.
        let work_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "tasks::retry::work")
            .expect("namespaced work step should exist");
        assert!(
            work_entry.prompt.contains("task=auth"),
            "for_each.task should resolve in nested do_while, got: {}",
            work_entry.prompt
        );
        assert!(
            work_entry.prompt.contains("iter=0"),
            "do_while.iteration should resolve, got: {}",
            work_entry.prompt
        );
    }
}

// ── YAML-based integration tests ──────────────────────────────────────────

mod yaml_integration {
    use ail_core::config;

    /// Full pipeline with output_schema + for_each parses correctly from YAML.
    #[test]
    fn schema_array_for_each_pipeline_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: plan
    prompt: "Break this into tasks. Respond with a JSON array."
    output_schema:
      type: array
      items:
        type: string
      maxItems: 20

  - id: implement
    for_each:
      over: "{{ step.plan.items }}"
      as: task
      max_items: 20
      on_max_items: abort_pipeline
      steps:
        - id: code
          prompt: "Implement: {{ for_each.task }}"
          resume: true
        - id: test
          context:
            shell: "cargo test 2>&1"

  - id: summary
    prompt: "Summarize what was done"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).expect("pipeline should parse");
        assert_eq!(pipeline.steps.len(), 3);

        // Plan step has output_schema.
        assert!(pipeline.steps[0].output_schema.is_some());

        // Second step is for_each.
        match &pipeline.steps[1].body {
            ail_core::config::domain::StepBody::ForEach {
                over,
                as_name,
                max_items,
                ..
            } => {
                assert_eq!(over, "{{ step.plan.items }}");
                assert_eq!(as_name, "task");
                assert_eq!(*max_items, Some(20));
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }

    /// do_while inside for_each parses correctly from YAML.
    #[test]
    fn nested_do_while_in_for_each_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: plan
    prompt: "plan tasks"
    output_schema:
      type: array
      items:
        type: string

  - id: tasks
    for_each:
      over: "{{ step.plan.items }}"
      as: task
      steps:
        - id: retry_loop
          do_while:
            max_iterations: 5
            exit_when: "{{ step.test.exit_code }} == 0"
            steps:
              - id: impl
                prompt: "implement {{ for_each.task }}"
                resume: true
              - id: test
                context:
                  shell: "cargo test 2>&1"
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).expect("nested pipeline should parse");
        assert_eq!(pipeline.steps.len(), 2);

        // Verify the nesting structure.
        match &pipeline.steps[1].body {
            ail_core::config::domain::StepBody::ForEach { steps, .. } => {
                assert_eq!(steps.len(), 1);
                match &steps[0].body {
                    ail_core::config::domain::StepBody::DoWhile {
                        max_iterations,
                        steps: inner,
                        ..
                    } => {
                        assert_eq!(*max_iterations, 5);
                        assert_eq!(inner.len(), 2);
                    }
                    other => panic!("expected DoWhile inside ForEach, got {other:?}"),
                }
            }
            other => panic!("expected ForEach, got {other:?}"),
        }
    }

    /// field:equals: with input_schema inside a for_each body parses from YAML.
    #[test]
    fn field_equals_inside_for_each_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: plan
    prompt: "plan"
    output_schema:
      type: array
      items:
        type: string

  - id: tasks
    for_each:
      over: "{{ step.plan.items }}"
      as: task
      steps:
        - id: classify
          prompt: "classify {{ for_each.task }}"
        - id: route
          prompt: "route"
          input_schema:
            type: object
            properties:
              priority:
                type: string
            required: [priority]
          on_result:
            field: priority
            equals: "high"
            if_true:
              action: continue
            if_false:
              action: break
"#;
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        let pipeline = config::load(tmp.path()).expect("field:equals: in for_each should parse");
        assert_eq!(pipeline.steps.len(), 2);
    }
}
