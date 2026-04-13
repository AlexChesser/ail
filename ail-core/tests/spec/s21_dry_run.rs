//! Tests for the dry-run mode (SPEC §21 — Dry Run Mode).
//!
//! Verifies that the `DryRunRunner` correctly bypasses LLM calls while
//! allowing the full pipeline resolution to proceed, including template
//! variable substitution and step ordering.

use ail_core::config::domain::{ContextSource, Pipeline, Step, StepBody, StepId};
use ail_core::executor::{execute, ExecuteOutcome};
use ail_core::runner::dry_run::DryRunRunner;
use ail_core::runner::InvokeOptions;
use ail_core::runner::Runner;
use ail_core::session::log_provider::NullProvider;
use ail_core::session::Session;
use ail_core::test_helpers::{make_session, prompt_step};

/// DryRunRunner returns a synthetic response without calling an LLM.
#[test]
fn dry_run_runner_returns_synthetic_response() {
    let runner = DryRunRunner::new();
    let result = runner
        .invoke("any prompt", InvokeOptions::default())
        .unwrap();
    assert!(
        result.response.contains("[DRY RUN]"),
        "response should contain [DRY RUN] marker"
    );
    assert_eq!(result.cost_usd, Some(0.0), "cost should be zero");
    assert_eq!(result.input_tokens, 0, "no tokens should be consumed");
    assert_eq!(result.output_tokens, 0, "no tokens should be consumed");
}

/// DryRunRunner can be used as a normal runner in the executor.
#[test]
fn dry_run_runner_executes_prompt_step() {
    let mut session = make_session(vec![prompt_step("review", "Review the code")]);
    let runner = DryRunRunner::new();
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "review");
    assert!(entries[0]
        .response
        .as_deref()
        .unwrap_or("")
        .contains("[DRY RUN]"));
}

/// DryRunRunner works with multi-step pipelines.
#[test]
fn dry_run_runner_executes_multi_step_pipeline() {
    let mut session = make_session(vec![
        prompt_step("step_a", "First"),
        prompt_step("step_b", "Second"),
        prompt_step("step_c", "Third"),
    ]);
    let runner = DryRunRunner::new();
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].step_id, "step_a");
    assert_eq!(entries[1].step_id, "step_b");
    assert_eq!(entries[2].step_id, "step_c");
}

/// Template variables resolve correctly with the DryRunRunner, since earlier steps
/// produce synthetic responses that can be referenced by later steps.
#[test]
fn dry_run_runner_template_variables_resolve() {
    let mut session = make_session(vec![
        prompt_step("first", "Hello"),
        prompt_step("second", "Previous response: {{ step.first.response }}"),
    ]);
    let runner = DryRunRunner::new();
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    // The second step's prompt should contain the dry-run response from step "first"
    assert!(
        entries[1].prompt.contains("[DRY RUN]"),
        "template variable should resolve to dry-run response: {}",
        entries[1].prompt
    );
}

/// Shell context steps execute normally during dry-run — they are local and free.
#[test]
fn dry_run_shell_context_steps_execute() {
    let shell_step = Step {
        id: StepId("check".to_string()),
        body: StepBody::Context(ContextSource::Shell("echo dry_run_test".to_string())),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
    };
    let mut session = make_session(vec![shell_step]);
    let runner = DryRunRunner::new();
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "check");
    // Shell steps execute normally — stdout should be populated
    assert!(
        entries[0]
            .stdout
            .as_deref()
            .unwrap_or("")
            .contains("dry_run_test"),
        "shell step should execute and produce stdout"
    );
    assert_eq!(entries[0].exit_code, Some(0));
}

/// DryRunRunner works with passthrough pipelines (invocation-only).
#[test]
fn dry_run_passthrough_pipeline() {
    let mut session = Session::new(Pipeline::passthrough(), "hello dry run".to_string())
        .with_log_provider(Box::new(NullProvider));
    let runner = DryRunRunner::new();
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "invocation");
    assert!(entries[0]
        .response
        .as_deref()
        .unwrap_or("")
        .contains("[DRY RUN]"));
}

/// Mixed prompt + context steps both work under dry-run.
#[test]
fn dry_run_mixed_prompt_and_context_pipeline() {
    let ctx = Step {
        id: StepId("ctx".to_string()),
        body: StepBody::Context(ContextSource::Shell("echo ok".to_string())),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
    };
    let mut session = make_session(vec![prompt_step("ask", "question"), ctx]);
    let runner = DryRunRunner::new();
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].step_id, "ask");
    assert!(entries[0].response.is_some());
    assert_eq!(entries[1].step_id, "ctx");
    assert_eq!(entries[1].exit_code, Some(0));
}
