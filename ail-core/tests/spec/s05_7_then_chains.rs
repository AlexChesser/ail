//! SPEC §5.7 — `then:` private post-processing chains.
//! SPEC §5.10 — `before:` private pre-processing chains.

use ail_core::config::domain::{ContextSource, Step, StepBody, StepId};
use ail_core::config::load;
use ail_core::executor::execute;
use ail_core::runner::stub::StubRunner;
use ail_core::test_helpers::{make_session, prompt_step};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ── YAML parsing ────────────────────────────────────────────────────────────

/// SPEC §5.7 — `then:` chain entries parse from YAML into domain Steps.
#[test]
fn then_chain_parses_from_yaml() {
    let pipeline = load(&fixtures_dir().join("before_then_chains.ail.yaml")).unwrap();
    assert_eq!(pipeline.steps.len(), 1);
    let step = &pipeline.steps[0];
    assert_eq!(step.id.as_str(), "main_step");
    assert_eq!(step.then.len(), 1);
    assert_eq!(step.then[0].id.as_str(), "main_step::then::0");
    assert!(matches!(step.then[0].body, StepBody::Prompt(_)));
}

/// SPEC §5.10 — `before:` chain entries parse from YAML into domain Steps.
#[test]
fn before_chain_parses_from_yaml() {
    let pipeline = load(&fixtures_dir().join("before_then_chains.ail.yaml")).unwrap();
    let step = &pipeline.steps[0];
    assert_eq!(step.before.len(), 1);
    assert_eq!(step.before[0].id.as_str(), "main_step::before::0");
    assert!(matches!(step.before[0].body, StepBody::Prompt(_)));
}

/// Nested chains parse correctly.
#[test]
fn nested_chains_parse_from_yaml() {
    let pipeline = load(&fixtures_dir().join("nested_chains.ail.yaml")).unwrap();
    let step = &pipeline.steps[0];

    // before chain has a nested then
    assert_eq!(step.before.len(), 1);
    assert_eq!(step.before[0].then.len(), 1);
    assert_eq!(
        step.before[0].then[0].id.as_str(),
        "outer::before::0::then::0"
    );

    // then chain has a nested before
    assert_eq!(step.then.len(), 1);
    assert_eq!(step.then[0].before.len(), 1);
    assert_eq!(
        step.then[0].before[0].id.as_str(),
        "outer::then::0::before::0"
    );
}

// ── Execution: before chains ────────────────────────────────────────────────

/// SPEC §5.10 — `before:` chain steps run before the parent step.
#[test]
fn before_chain_runs_before_parent() {
    let mut parent = prompt_step("parent", "main work");
    parent.before = vec![Step {
        id: StepId("parent::before::0".to_string()),
        body: StepBody::Prompt("pre-process".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    // before chain step runs first, then parent step
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].step_id, "parent::before::0");
    assert_eq!(entries[1].step_id, "parent");
}

// ── Execution: then chains ──────────────────────────────────────────────────

/// SPEC §5.7 — `then:` chain steps run after the parent step.
#[test]
fn then_chain_runs_after_parent() {
    let mut parent = prompt_step("parent", "main work");
    parent.then = vec![Step {
        id: StepId("parent::then::0".to_string()),
        body: StepBody::Prompt("post-process".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    // parent step runs first, then the then chain step
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].step_id, "parent");
    assert_eq!(entries[1].step_id, "parent::then::0");
}

// ── Execution: both before and then ─────────────────────────────────────────

/// SPEC §5.6 — Full lifecycle: before → parent → then.
#[test]
fn before_then_lifecycle_order() {
    let mut parent = prompt_step("parent", "main work");
    parent.before = vec![Step {
        id: StepId("parent::before::0".to_string()),
        body: StepBody::Prompt("before step".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];
    parent.then = vec![Step {
        id: StepId("parent::then::0".to_string()),
        body: StepBody::Prompt("then step".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].step_id, "parent::before::0");
    assert_eq!(entries[1].step_id, "parent");
    assert_eq!(entries[2].step_id, "parent::then::0");
}

// ── Template variables from parent scope ────────────────────────────────────

/// Chain steps have access to template variables from parent scope —
/// a then: step can access the parent step's response.
#[test]
fn then_chain_accesses_parent_response_via_template() {
    let mut parent = prompt_step("parent", "main work");
    parent.then = vec![Step {
        id: StepId("parent::then::0".to_string()),
        body: StepBody::Prompt("Result was: {{ step.parent.response }}".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("parent-output");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    // The then chain step's prompt should have resolved {{ step.parent.response }}
    assert!(
        entries[1].prompt.contains("parent-output"),
        "then chain step should have access to parent's response, got prompt: {}",
        entries[1].prompt
    );
}

/// Chain steps can reference earlier chain step results via template variables.
#[test]
fn before_chain_result_accessible_via_template() {
    let mut parent = prompt_step("parent", "Previous: {{ step.parent::before::0.response }}");
    parent.before = vec![Step {
        id: StepId("parent::before::0".to_string()),
        body: StepBody::Prompt("pre-process".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("before-result");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    // Parent prompt should contain the before step's response
    assert!(
        entries[1].prompt.contains("before-result"),
        "parent step should access before chain step's response, got prompt: {}",
        entries[1].prompt
    );
}

// ── Nested chains ───────────────────────────────────────────────────────────

/// Nested chains execute in the correct order:
/// before's before → before → parent → then → then's then.
#[test]
fn nested_chains_execute_in_correct_order() {
    let mut parent = prompt_step("parent", "main");

    // before chain step has its own before chain
    let mut before_step = Step {
        id: StepId("parent::before::0".to_string()),
        body: StepBody::Prompt("before-main".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![Step {
            id: StepId("parent::before::0::before::0".to_string()),
            body: StepBody::Prompt("before-before".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            async_step: false,
            depends_on: vec![],
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
            input_schema: None,
        }],
        then: vec![],
        output_schema: None,
        input_schema: None,
    };

    // then chain step has its own then chain
    let then_step = Step {
        id: StepId("parent::then::0".to_string()),
        body: StepBody::Prompt("then-main".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![Step {
            id: StepId("parent::then::0::then::0".to_string()),
            body: StepBody::Prompt("then-then".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            async_step: false,
            depends_on: vec![],
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
            input_schema: None,
        }],
        output_schema: None,
        input_schema: None,
    };

    parent.before = vec![before_step];
    parent.then = vec![then_step];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(
        entries.len(),
        5,
        "Expected 5 entries: before-before, before-main, parent, then-main, then-then"
    );
    assert_eq!(entries[0].step_id, "parent::before::0::before::0");
    assert_eq!(entries[1].step_id, "parent::before::0");
    assert_eq!(entries[2].step_id, "parent");
    assert_eq!(entries[3].step_id, "parent::then::0");
    assert_eq!(entries[4].step_id, "parent::then::0::then::0");
}

// ── Context steps in chains ─────────────────────────────────────────────────

/// Context shell steps work in chains — their results are accessible via template vars.
#[test]
fn context_step_in_before_chain_works() {
    let mut parent = prompt_step("parent", "Lint result: {{ step.parent::before::0.result }}");
    parent.before = vec![Step {
        id: StepId("parent::before::0".to_string()),
        body: StepBody::Context(ContextSource::Shell("echo lint-passed".to_string())),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].step_id, "parent::before::0");
    assert!(entries[0]
        .stdout
        .as_deref()
        .unwrap_or("")
        .contains("lint-passed"));
    // Parent step prompt should contain the shell output
    assert!(
        entries[1].prompt.contains("lint-passed"),
        "parent should access before chain context result, got: {}",
        entries[1].prompt
    );
}

// ── Multiple chain steps ────────────────────────────────────────────────────

/// Multiple before chain steps run in order.
#[test]
fn multiple_before_chain_steps_run_in_order() {
    let mut parent = prompt_step("parent", "main");
    parent.before = vec![
        Step {
            id: StepId("parent::before::0".to_string()),
            body: StepBody::Prompt("first before".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            async_step: false,
            depends_on: vec![],
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
            input_schema: None,
        },
        Step {
            id: StepId("parent::before::1".to_string()),
            body: StepBody::Prompt("second before".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            async_step: false,
            depends_on: vec![],
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
            input_schema: None,
        },
    ];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].step_id, "parent::before::0");
    assert_eq!(entries[1].step_id, "parent::before::1");
    assert_eq!(entries[2].step_id, "parent");
}

/// Multiple then chain steps run in order.
#[test]
fn multiple_then_chain_steps_run_in_order() {
    let mut parent = prompt_step("parent", "main");
    parent.then = vec![
        Step {
            id: StepId("parent::then::0".to_string()),
            body: StepBody::Prompt("first then".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            async_step: false,
            depends_on: vec![],
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
            input_schema: None,
        },
        Step {
            id: StepId("parent::then::1".to_string()),
            body: StepBody::Prompt("second then".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            async_step: false,
            depends_on: vec![],
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
            input_schema: None,
        },
    ];

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].step_id, "parent");
    assert_eq!(entries[1].step_id, "parent::then::0");
    assert_eq!(entries[2].step_id, "parent::then::1");
}

// ── Materialize ─────────────────────────────────────────────────────────────

/// SPEC §5.7 / §5.10 — materialize output shows before/then chains as comments.
#[test]
fn materialize_shows_before_then_chains() {
    use ail_core::config::domain::{Pipeline, ProviderConfig};
    use ail_core::materialize::materialize;

    let mut step = prompt_step("review", "Review the code.");
    step.before = vec![Step {
        id: StepId("review::before::0".to_string()),
        body: StepBody::Prompt("Gather context.".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];
    step.then = vec![Step {
        id: StepId("review::then::0".to_string()),
        body: StepBody::Prompt("Summarize findings.".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
    }];

    let pipeline = Pipeline {
        steps: vec![step],
        source: Some(PathBuf::from("test.ail.yaml")),
        defaults: ProviderConfig::default(),
        timeout_seconds: None,
        default_tools: None,
        named_pipelines: Default::default(),
        max_concurrency: None,
    };

    let output = materialize(&pipeline);

    assert!(
        output.contains("# before: (private"),
        "Expected before comment in materialize output, got:\n{output}"
    );
    assert!(
        output.contains("review::before::0"),
        "Expected before chain step id in materialize output, got:\n{output}"
    );
    assert!(
        output.contains("# then: (private"),
        "Expected then comment in materialize output, got:\n{output}"
    );
    assert!(
        output.contains("review::then::0"),
        "Expected then chain step id in materialize output, got:\n{output}"
    );
}

// ── YAML short-form parsing ─────────────────────────────────────────────────

/// Short-form (bare string) entries in then: parse as prompt steps.
#[test]
fn short_form_then_entry_parses_as_prompt() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: main
    prompt: "Do work."
    then:
      - "Summarize the output."
"#;
    let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
    std::fs::write(tmp.path(), yaml).unwrap();
    let pipeline = load(tmp.path()).unwrap();
    assert_eq!(pipeline.steps[0].then.len(), 1);
    assert_eq!(pipeline.steps[0].then[0].id.as_str(), "main::then::0");
    if let StepBody::Prompt(text) = &pipeline.steps[0].then[0].body {
        assert_eq!(text, "Summarize the output.");
    } else {
        panic!("Expected Prompt body for short-form then entry");
    }
}

/// Short-form (bare string) entries in before: parse as prompt steps.
#[test]
fn short_form_before_entry_parses_as_prompt() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: main
    prompt: "Do work."
    before:
      - "Gather context first."
"#;
    let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
    std::fs::write(tmp.path(), yaml).unwrap();
    let pipeline = load(tmp.path()).unwrap();
    assert_eq!(pipeline.steps[0].before.len(), 1);
    assert_eq!(pipeline.steps[0].before[0].id.as_str(), "main::before::0");
    if let StepBody::Prompt(text) = &pipeline.steps[0].before[0].body {
        assert_eq!(text, "Gather context first.");
    } else {
        panic!("Expected Prompt body for short-form before entry");
    }
}

/// Mixed short-form and full-form entries parse correctly.
#[test]
fn mixed_short_full_form_entries_parse() {
    let yaml = r#"
version: "0.1"
pipeline:
  - id: main
    prompt: "Do work."
    then:
      - "Short form cleanup."
      - prompt: "Full form analysis."
"#;
    let tmp = tempfile::NamedTempFile::with_suffix(".ail.yaml").unwrap();
    std::fs::write(tmp.path(), yaml).unwrap();
    let pipeline = load(tmp.path()).unwrap();
    assert_eq!(pipeline.steps[0].then.len(), 2);

    if let StepBody::Prompt(text) = &pipeline.steps[0].then[0].body {
        assert_eq!(text, "Short form cleanup.");
    } else {
        panic!("Expected Prompt body for short-form entry");
    }

    if let StepBody::Prompt(text) = &pipeline.steps[0].then[1].body {
        assert_eq!(text, "Full form analysis.");
    } else {
        panic!("Expected Prompt body for full-form entry");
    }
}

// ── Empty chains ────────────────────────────────────────────────────────────

/// Steps with no before/then chains work as before (no regression).
#[test]
fn step_without_chains_works_unchanged() {
    let parent = prompt_step("parent", "main work");
    assert!(parent.before.is_empty());
    assert!(parent.then.is_empty());

    let mut session = make_session(vec![parent]);
    let runner = StubRunner::new("ok");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "parent");
}
