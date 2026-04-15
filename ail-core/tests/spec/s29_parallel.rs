//! SPEC §29 — parallel step execution.

// ── Parse-time validation ───────────────────────────────────────────────────

mod parse {
    use ail_core::config;
    use ail_core::config::domain::{ActionKind, JoinErrorMode, StepBody};

    fn load(yaml: &str) -> Result<config::domain::Pipeline, ail_core::error::AilError> {
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        config::load(tmp.path())
    }

    #[test]
    fn basic_async_plus_join_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: lint
    async: true
    prompt: "Run lint"
  - id: test
    async: true
    prompt: "Run tests"
  - id: done
    depends_on: [lint, test]
    action: join
"#;
        let pipeline = load(yaml).expect("should parse");
        assert!(pipeline.steps[0].async_step);
        assert!(pipeline.steps[1].async_step);
        assert_eq!(pipeline.steps[2].depends_on.len(), 2);
        assert!(matches!(
            pipeline.steps[2].body,
            StepBody::Action(ActionKind::Join { .. })
        ));
    }

    #[test]
    fn join_wait_for_all_mode_parses() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    prompt: "a"
  - id: done
    depends_on: [a]
    action: join
    on_error: wait_for_all
"#;
        let pipeline = load(yaml).expect("should parse");
        if let StepBody::Action(ActionKind::Join { on_error_mode }) = &pipeline.steps[1].body {
            assert_eq!(*on_error_mode, JoinErrorMode::WaitForAll);
        } else {
            panic!("expected Join body");
        }
    }

    #[test]
    fn max_concurrency_parses() {
        let yaml = r#"
version: "0.1"
defaults:
  max_concurrency: 4
pipeline:
  - id: a
    async: true
    prompt: "a"
  - id: done
    depends_on: [a]
    action: join
"#;
        let pipeline = load(yaml).expect("should parse");
        assert_eq!(pipeline.max_concurrency, Some(4));
    }

    #[test]
    fn orphaned_async_step_is_parse_error() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: orphan
    async: true
    prompt: "unreferenced"
  - id: ship
    prompt: "done"
"#;
        let err = load(yaml).expect_err("should reject orphaned async");
        assert!(
            err.detail().contains("not named in any step's depends_on"),
            "got: {}",
            err.detail()
        );
    }

    #[test]
    fn join_without_depends_on_is_parse_error() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: bad
    action: join
"#;
        let err = load(yaml).expect_err("should reject join without depends_on");
        assert!(
            err.detail().contains("no depends_on list"),
            "got: {}",
            err.detail()
        );
    }

    #[test]
    fn forward_reference_in_depends_on_is_parse_error() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: first
    depends_on: [later]
    action: join
  - id: later
    async: true
    prompt: "x"
"#;
        let err = load(yaml).expect_err("should reject forward ref");
        // The error can come from either forward-ref check or cycle detection;
        // both are valid — the point is the pipeline is rejected.
        let d = err.detail();
        assert!(
            d.contains("declared later") || d.contains("not declared"),
            "got: {d}"
        );
    }

    #[test]
    fn cycle_in_depends_on_is_parse_error() {
        // Both steps forward-ref each other — caught as forward-ref first.
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    depends_on: [b]
    prompt: "a"
  - id: b
    depends_on: [a]
    prompt: "b"
"#;
        let err = load(yaml).expect_err("should reject");
        assert!(!err.detail().is_empty());
    }

    #[test]
    fn concurrent_resume_conflict_is_parse_error() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    resume: true
    prompt: "a"
  - id: b
    async: true
    resume: true
    prompt: "b"
  - id: done
    depends_on: [a, b]
    action: join
"#;
        let err = load(yaml).expect_err("should reject concurrent resume conflict");
        assert!(
            err.detail().contains("cannot share a runner session"),
            "got: {}",
            err.detail()
        );
    }

    #[test]
    fn structured_join_mixed_dependencies_is_parse_error() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    prompt: "a"
    output_schema:
      type: object
      properties:
        ok: { type: boolean }
      required: [ok]
  - id: b
    async: true
    prompt: "b"
  - id: done
    depends_on: [a, b]
    action: join
    output_schema:
      type: object
      properties:
        a: { type: object }
        b: { type: object }
"#;
        let err = load(yaml).expect_err("should reject mixed structured/unstructured");
        assert!(
            err.detail().contains("does not declare output_schema"),
            "got: {}",
            err.detail()
        );
    }
}

// ── Runtime execution ───────────────────────────────────────────────────────

mod execution {
    use ail_core::config::domain::{Pipeline, Step, StepBody, StepId};
    use ail_core::executor::execute;
    use ail_core::runner::stub::{RecordingStubRunner, StubRunner};
    use ail_core::session::log_provider::NullProvider;
    use ail_core::session::Session;

    fn prompt_step(id: &str, text: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt(text.to_string()),
            ..Default::default()
        }
    }

    fn make_session(pipeline: Pipeline) -> Session {
        Session::new(pipeline, "invocation".to_string()).with_log_provider(Box::new(NullProvider))
    }

    fn load_inline(yaml: &str) -> Pipeline {
        let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
        std::fs::write(tmp.path(), yaml).unwrap();
        ail_core::config::load(tmp.path()).expect("valid pipeline")
    }

    #[test]
    fn two_async_steps_and_join_produce_merged_entry() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    prompt: "run a"
  - id: b
    async: true
    prompt: "run b"
  - id: done
    depends_on: [a, b]
    action: join
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = StubRunner::new("ok");
        execute(&mut session, &runner).expect("pipeline runs");

        let entries = session.turn_log.entries();
        let ids: Vec<&str> = entries.iter().map(|e| e.step_id.as_str()).collect();
        assert!(ids.contains(&"a"), "missing branch a in {ids:?}");
        assert!(ids.contains(&"b"), "missing branch b in {ids:?}");
        assert!(ids.contains(&"done"), "missing join in {ids:?}");

        // Branch entries carry concurrent_group metadata.
        for id in &["a", "b"] {
            let e = entries.iter().find(|e| e.step_id == *id).unwrap();
            assert!(
                e.concurrent_group.is_some(),
                "branch {id} should have concurrent_group"
            );
            assert!(e.launched_at.is_some());
            assert!(e.completed_at.is_some());
        }

        // Join entry's response is the string-join concatenation.
        let join_entry = entries.iter().find(|e| e.step_id == "done").unwrap();
        let resp = join_entry.response.as_deref().unwrap_or("");
        assert!(resp.contains("[a]:"), "missing [a] label in {resp}");
        assert!(resp.contains("[b]:"), "missing [b] label in {resp}");
        assert!(resp.contains("ok"), "missing branch response in {resp}");
    }

    #[test]
    fn both_async_branches_invoke_runner() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    prompt: "branch a"
  - id: b
    async: true
    prompt: "branch b"
  - id: done
    depends_on: [a, b]
    action: join
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = RecordingStubRunner::new("ok");
        execute(&mut session, &runner).expect("pipeline runs");

        let calls = runner.calls();
        // Two async branches → exactly 2 runner invocations.
        assert_eq!(calls.len(), 2, "expected 2 calls, got {}", calls.len());
        let prompts: Vec<&str> = calls.iter().map(|c| c.prompt.as_str()).collect();
        assert!(prompts.contains(&"branch a"), "got {prompts:?}");
        assert!(prompts.contains(&"branch b"), "got {prompts:?}");
    }

    #[test]
    fn string_join_preserves_declaration_order() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: first
    async: true
    prompt: "1"
  - id: second
    async: true
    prompt: "2"
  - id: third
    async: true
    prompt: "3"
  - id: done
    depends_on: [first, second, third]
    action: join
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = StubRunner::new("resp");
        execute(&mut session, &runner).expect("pipeline runs");

        let join_entry = session
            .turn_log
            .entries()
            .iter()
            .find(|e| e.step_id == "done")
            .cloned()
            .unwrap();
        let resp = join_entry.response.unwrap();
        let first_pos = resp.find("[first]:").unwrap();
        let second_pos = resp.find("[second]:").unwrap();
        let third_pos = resp.find("[third]:").unwrap();
        assert!(first_pos < second_pos);
        assert!(second_pos < third_pos);
    }

    #[test]
    fn sequential_step_after_async_sees_join_result() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    prompt: "a"
  - id: done
    depends_on: [a]
    action: join
  - id: after
    prompt: "received: {{ step.done.response }}"
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = RecordingStubRunner::new("branch response");
        execute(&mut session, &runner).expect("pipeline runs");

        let calls = runner.calls();
        // 1 async branch + 1 sequential after = 2 invocations.
        let after_call = calls.iter().find(|c| c.prompt.starts_with("received:"));
        let after = after_call.expect("after step should have been invoked");
        assert!(
            after.prompt.contains("[a]:"),
            "join response not substituted into after step: {}",
            after.prompt
        );
    }

    #[test]
    fn condition_never_on_async_step_skips_but_unblocks_join() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    condition: never
    prompt: "a"
  - id: b
    async: true
    prompt: "b"
  - id: done
    depends_on: [a, b]
    action: join
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = RecordingStubRunner::new("ok");
        execute(&mut session, &runner).expect("pipeline runs");

        let calls = runner.calls();
        // Only b runs — a is condition-skipped.
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].prompt, "b");

        // Join still completes.
        let entries = session.turn_log.entries();
        assert!(entries.iter().any(|e| e.step_id == "done"));
    }

    #[test]
    fn on_result_contains_on_join_step_fires() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: lint
    async: true
    prompt: "lint"
  - id: done
    depends_on: [lint]
    action: join
    on_result:
      - contains: "FAIL"
        action: abort_pipeline
      - always: true
        action: continue
  - id: ship
    prompt: "ship"
"#;
        let pipeline = load_inline(yaml);

        // Case A: no FAIL → ship runs.
        let mut s1 = make_session(pipeline.clone());
        let ok_runner = StubRunner::new("all good");
        execute(&mut s1, &ok_runner).expect("pipeline runs");
        assert!(s1.turn_log.entries().iter().any(|e| e.step_id == "ship"));

        // Case B: FAIL → aborts before ship.
        let mut s2 = make_session(pipeline);
        let fail_runner = StubRunner::new("FAIL: broken");
        let res = execute(&mut s2, &fail_runner);
        assert!(res.is_err(), "expected abort; got {res:?}");
        assert!(!s2.turn_log.entries().iter().any(|e| e.step_id == "ship"));
    }

    #[test]
    fn sequential_only_pipeline_still_works() {
        // Guard against regressions in the non-async path.
        let pipeline = Pipeline {
            steps: vec![prompt_step("a", "hello"), prompt_step("b", "world")],
            ..Default::default()
        };
        let mut session = make_session(pipeline);
        let runner = StubRunner::new("ok");
        execute(&mut session, &runner).expect("pipeline runs");
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].step_id, "a");
        assert_eq!(entries[1].step_id, "b");
    }

    #[test]
    fn max_concurrency_1_serializes_branches() {
        // max_concurrency: 1 forces branches to run one-at-a-time. The
        // result should still be correct.
        let yaml = r#"
version: "0.1"
defaults:
  max_concurrency: 1
pipeline:
  - id: a
    async: true
    prompt: "a"
  - id: b
    async: true
    prompt: "b"
  - id: c
    async: true
    prompt: "c"
  - id: done
    depends_on: [a, b, c]
    action: join
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = RecordingStubRunner::new("ok");
        execute(&mut session, &runner).expect("pipeline runs");
        let calls = runner.calls();
        assert_eq!(calls.len(), 3);
    }

    #[test]
    fn turn_log_concurrent_group_shared_across_branches() {
        let yaml = r#"
version: "0.1"
pipeline:
  - id: a
    async: true
    prompt: "a"
  - id: b
    async: true
    prompt: "b"
  - id: done
    depends_on: [a, b]
    action: join
"#;
        let pipeline = load_inline(yaml);
        let mut session = make_session(pipeline);
        let runner = StubRunner::new("ok");
        execute(&mut session, &runner).expect("pipeline runs");

        let entries = session.turn_log.entries();
        let a_group = entries
            .iter()
            .find(|e| e.step_id == "a")
            .and_then(|e| e.concurrent_group.clone())
            .unwrap();
        let b_group = entries
            .iter()
            .find(|e| e.step_id == "b")
            .and_then(|e| e.concurrent_group.clone())
            .unwrap();
        assert_eq!(a_group, b_group, "branches should share concurrent_group");
    }
}

// ── Integration: fixtures round-trip ────────────────────────────────────────

mod fixtures {
    use ail_core::config;

    #[test]
    fn parallel_basic_fixture_parses() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/parallel_basic.ail.yaml");
        let p = config::load(&path).expect("should parse");
        assert_eq!(p.steps.len(), 4);
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn parallel_structured_fixture_parses() {
        let p = config::load(&fixture("parallel_structured.ail.yaml")).expect("should parse");
        assert_eq!(p.steps.len(), 3);
    }

    #[test]
    fn parallel_invalid_orphan_fixture_rejected() {
        let res = config::load(&fixture("parallel_invalid_orphan.ail.yaml"));
        assert!(res.is_err());
    }

    #[test]
    fn parallel_invalid_join_no_deps_fixture_rejected() {
        let res = config::load(&fixture("parallel_invalid_join_no_deps.ail.yaml"));
        assert!(res.is_err());
    }

    #[test]
    fn parallel_invalid_forward_ref_fixture_rejected() {
        let res = config::load(&fixture("parallel_invalid_forward_ref.ail.yaml"));
        assert!(res.is_err());
    }
}
