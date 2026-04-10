//! Headless (non-interactive) pipeline execution — `execute()` and its recursive inner loop.

#![allow(clippy::result_large_err)]

use std::time::SystemTime;

use crate::config::domain::{
    Condition, ContextSource, ResultAction, StepBody, MAX_SUB_PIPELINE_DEPTH,
};
use crate::error::{error_types, AilError};
use crate::runner::{InvokeOptions, Runner};
use crate::session::{Session, TurnEntry};
use crate::template;

use super::events::ExecuteOutcome;
use super::helpers::{
    build_step_runner_box, build_tool_policy, evaluate_on_result, resolve_prompt_file,
    resolve_step_provider, run_shell_command,
};

/// Load and run a sub-pipeline, returning a `TurnEntry` for the calling step.
///
/// The `path_template` may contain `{{ variable }}` syntax (SPEC §11); it is resolved
/// against `session` before the file is loaded. The sub-pipeline runs in isolation:
/// a fresh `Session` is created with the parent's `last_response` as its invocation prompt.
/// The child's final step response becomes the returned entry's `response` field.
///
/// `depth` guards against infinite recursion; exceeding `MAX_SUB_PIPELINE_DEPTH` aborts.
pub(super) fn execute_sub_pipeline(
    path_template: &str,
    prompt_override: Option<&str>,
    step_id: &str,
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
    base_dir: Option<&std::path::Path>,
) -> Result<TurnEntry, AilError> {
    if depth >= MAX_SUB_PIPELINE_DEPTH {
        return Err(AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Sub-pipeline depth limit exceeded",
            detail: format!(
                "Step '{step_id}' would exceed the maximum sub-pipeline nesting depth of {MAX_SUB_PIPELINE_DEPTH}"
            ),
            context: Some(crate::error::ErrorContext {
                pipeline_run_id: Some(session.run_id.clone()),
                step_id: Some(step_id.to_string()),
                source: None,
            }),
        });
    }

    // Resolve template variables in the path (SPEC §11).
    let resolved_path = template::resolve(path_template, session).map_err(|mut e| {
        e.context = Some(crate::error::ErrorContext {
            pipeline_run_id: Some(session.run_id.clone()),
            step_id: Some(step_id.to_string()),
            source: None,
        });
        e
    })?;

    // Resolve ./relative and ../relative paths against the parent pipeline's directory (SPEC §9).
    let path_buf = if let (true, Some(base)) = (
        resolved_path.starts_with("./") || resolved_path.starts_with("../"),
        base_dir,
    ) {
        base.join(&resolved_path)
    } else {
        std::path::PathBuf::from(&resolved_path)
    };
    let path = path_buf.as_path();

    let sub_pipeline = crate::config::load(path).map_err(|mut e| {
        e.context = Some(crate::error::ErrorContext {
            pipeline_run_id: Some(session.run_id.clone()),
            step_id: Some(step_id.to_string()),
            source: None,
        });
        e
    })?;

    // The sub-pipeline's invocation prompt: use the explicit override when provided
    // (template-resolved against the parent session), otherwise fall back to the
    // parent's most recent response (SPEC §9).
    let invocation_prompt = if let Some(override_template) = prompt_override {
        template::resolve(override_template, session).map_err(|mut e| {
            e.context = Some(crate::error::ErrorContext {
                pipeline_run_id: Some(session.run_id.clone()),
                step_id: Some(step_id.to_string()),
                source: None,
            });
            e
        })?
    } else {
        session
            .turn_log
            .last_response()
            .unwrap_or(&session.invocation_prompt)
            .to_string()
    };

    let mut child_session = crate::session::Session::new(sub_pipeline, invocation_prompt);
    child_session.cli_provider = session.cli_provider.clone();

    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        sub_pipeline = %resolved_path,
        depth,
        "executing sub-pipeline"
    );

    execute_inner(&mut child_session, runner, depth + 1)?;

    let response = child_session
        .turn_log
        .last_response()
        .unwrap_or("")
        .to_string();

    Ok(TurnEntry {
        step_id: step_id.to_string(),
        prompt: resolved_path,
        response: Some(response),
        timestamp: SystemTime::now(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        runner_session_id: child_session
            .turn_log
            .last_runner_session_id()
            .map(str::to_string),
        stdout: None,
        stderr: None,
        exit_code: None,
        thinking: None,
        tool_events: vec![],
    })
}

/// Inner recursive executor used by both `execute()` and sub-pipeline calls.
/// `depth` tracks nesting level to enforce `MAX_SUB_PIPELINE_DEPTH`.
pub(super) fn execute_inner(
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
) -> Result<ExecuteOutcome, AilError> {
    if session.pipeline.steps.is_empty() {
        tracing::info!(run_id = %session.run_id, "empty pipeline — no steps to execute");
        return Ok(ExecuteOutcome::Completed);
    }

    // Clone to avoid borrow conflict when calling execute_sub_pipeline (&mut session)
    // while iterating step bodies.
    let steps: Vec<_> = session.pipeline.steps.clone();

    for step in &steps {
        let step_id = step.id.as_str().to_string();

        // Check condition — skip the step if condition is Never (SPEC §12).
        if step.condition == Some(Condition::Never) {
            tracing::info!(run_id = %session.run_id, step_id = %step_id, "step skipped by condition: never");
            continue;
        }

        tracing::info!(run_id = %session.run_id, step_id = %step_id, "executing step");

        // Base dir for resolving ./relative file paths — the pipeline file's parent dir (SPEC §5.2).
        // Owned PathBuf so we can pass it to execute_sub_pipeline without holding a borrow on session.
        let pipeline_base_dir_buf: Option<std::path::PathBuf> = session
            .pipeline
            .source
            .as_deref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        let pipeline_base_dir = pipeline_base_dir_buf.as_deref();

        let entry = match &step.body {
            StepBody::Prompt(template_text) => {
                let template_text =
                    resolve_prompt_file(template_text, &step_id, pipeline_base_dir)?;
                let resolved = template::resolve(&template_text, session).map_err(|mut e| {
                    e.context = Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id.clone()),
                        source: None,
                    });
                    e
                })?;

                let resume_id = if step.resume {
                    session
                        .turn_log
                        .last_runner_session_id()
                        .map(|s| s.to_string())
                } else {
                    None
                };
                session.turn_log.record_step_started(&step_id, &resolved);

                // Resolve system_prompt if set
                let resolved_system_prompt = step
                    .system_prompt
                    .as_deref()
                    .map(|sp| {
                        let content = resolve_prompt_file(sp, &step_id, pipeline_base_dir)?;
                        template::resolve(&content, session).map_err(|mut e| {
                            e.context = Some(crate::error::ErrorContext {
                                pipeline_run_id: Some(session.run_id.clone()),
                                step_id: Some(step_id.clone()),
                                source: None,
                            });
                            e
                        })
                    })
                    .transpose()?;

                // Resolve append_system_prompt entries
                let mut resolved_append_system_prompt: Vec<String> = Vec::new();
                if let Some(entries) = &step.append_system_prompt {
                    for entry in entries {
                        let text = match entry {
                            crate::config::domain::SystemPromptEntry::Text(s) => {
                                template::resolve(s, session).map_err(|mut e| {
                                    e.context = Some(crate::error::ErrorContext {
                                        pipeline_run_id: Some(session.run_id.clone()),
                                        step_id: Some(step_id.clone()),
                                        source: None,
                                    });
                                    e
                                })?
                            }
                            crate::config::domain::SystemPromptEntry::File(path) => {
                                let content = std::fs::read_to_string(path).map_err(|e| AilError {
                                    error_type: error_types::CONFIG_FILE_NOT_FOUND,
                                    title: "append_system_prompt file not found",
                                    detail: format!(
                                        "Step '{step_id}' append_system_prompt file '{}' could not be read: {e}",
                                        path.display()
                                    ),
                                    context: Some(crate::error::ErrorContext {
                                        pipeline_run_id: Some(session.run_id.clone()),
                                        step_id: Some(step_id.clone()),
                                        source: None,
                                    }),
                                })?;
                                template::resolve(&content, session).map_err(|mut e| {
                                    e.context = Some(crate::error::ErrorContext {
                                        pipeline_run_id: Some(session.run_id.clone()),
                                        step_id: Some(step_id.clone()),
                                        source: None,
                                    });
                                    e
                                })?
                            }
                            crate::config::domain::SystemPromptEntry::Shell(cmd) => {
                                let resolved_cmd =
                                    template::resolve(cmd, session).map_err(|mut e| {
                                        e.context = Some(crate::error::ErrorContext {
                                            pipeline_run_id: Some(session.run_id.clone()),
                                            step_id: Some(step_id.clone()),
                                            source: None,
                                        });
                                        e
                                    })?;
                                let (stdout, _stderr, _exit_code) =
                                    run_shell_command(&session.run_id, &step_id, &resolved_cmd)?;
                                stdout
                            }
                        };
                        resolved_append_system_prompt.push(text);
                    }
                }

                let resolved_provider = resolve_step_provider(session, step);
                let effective_tools = step
                    .tools
                    .as_ref()
                    .or(session.pipeline.default_tools.as_ref());
                let step_runner_box = build_step_runner_box(step)?;
                let effective_runner: &dyn Runner = step_runner_box
                    .as_deref()
                    .map(|b| b as &dyn Runner)
                    .unwrap_or(runner);

                let extensions = effective_runner.build_extensions(&resolved_provider);
                let options = InvokeOptions {
                    resume_session_id: resume_id,
                    tool_policy: build_tool_policy(effective_tools),
                    model: resolved_provider.model,
                    extensions,
                    permission_responder: None,
                    cancel_token: None,
                    system_prompt: resolved_system_prompt,
                    append_system_prompt: resolved_append_system_prompt,
                };

                let result = effective_runner
                    .invoke(&resolved, options)
                    .map_err(|mut e| {
                        e.context = Some(crate::error::ErrorContext {
                            pipeline_run_id: Some(session.run_id.clone()),
                            step_id: Some(step_id.clone()),
                            source: None,
                        });
                        e
                    })?;

                tracing::info!(run_id = %session.run_id, step_id = %step_id, cost_usd = ?result.cost_usd, "step complete");

                TurnEntry {
                    step_id: step_id.clone(),
                    prompt: resolved,
                    response: Some(result.response),
                    timestamp: SystemTime::now(),
                    cost_usd: result.cost_usd,
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
                    runner_session_id: result.session_id,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    thinking: result.thinking,
                    tool_events: result.tool_events,
                }
            }

            StepBody::Context(ContextSource::Shell(cmd)) => {
                session.turn_log.record_step_started(&step_id, cmd);
                let (stdout, stderr, exit_code) =
                    run_shell_command(&session.run_id, &step_id, cmd)?;
                tracing::info!(run_id = %session.run_id, step_id = %step_id, exit_code, "context shell step complete");
                TurnEntry {
                    step_id: step_id.clone(),
                    prompt: cmd.clone(),
                    response: None,
                    timestamp: SystemTime::now(),
                    cost_usd: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    runner_session_id: None,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    exit_code: Some(exit_code),
                    thinking: None,
                    tool_events: vec![],
                }
            }

            StepBody::Action(crate::config::domain::ActionKind::PauseForHuman) => {
                tracing::info!(run_id = %session.run_id, step_id = %step_id, "pause_for_human");
                // v0.1: pause_for_human is a no-op in headless/--once mode.
                continue;
            }

            StepBody::SubPipeline {
                path: path_template,
                prompt,
            } => {
                session
                    .turn_log
                    .record_step_started(&step_id, path_template);
                execute_sub_pipeline(
                    path_template,
                    prompt.as_deref(),
                    &step_id,
                    session,
                    runner,
                    depth,
                    pipeline_base_dir,
                )?
            }

            StepBody::Skill(_) => {
                return Err(AilError {
                    error_type: error_types::PIPELINE_ABORTED,
                    title: "Unsupported step type",
                    detail: format!(
                        "Step '{step_id}' uses a step type not yet implemented in v0.1"
                    ),
                    context: Some(crate::error::ErrorContext {
                        pipeline_run_id: Some(session.run_id.clone()),
                        step_id: Some(step_id),
                        source: None,
                    }),
                });
            }
        };

        session.turn_log.append(entry);

        // Evaluate on_result branches after every step that produced an entry.
        if let Some(branches) = &step.on_result {
            let last_entry = session.turn_log.entries().last().expect("just appended");
            if let Some(action) = evaluate_on_result(branches, last_entry) {
                match action {
                    ResultAction::Continue => {}
                    ResultAction::Break => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result break — stopping pipeline early"
                        );
                        return Ok(ExecuteOutcome::Break {
                            step_id: step_id.clone(),
                        });
                    }
                    ResultAction::AbortPipeline => {
                        return Err(AilError {
                            error_type: error_types::PIPELINE_ABORTED,
                            title: "Pipeline aborted by on_result",
                            detail: format!("Step '{step_id}' on_result fired abort_pipeline"),
                            context: Some(crate::error::ErrorContext {
                                pipeline_run_id: Some(session.run_id.clone()),
                                step_id: Some(step_id),
                                source: None,
                            }),
                        });
                    }
                    ResultAction::PauseForHuman => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            "on_result pause_for_human (no-op in uncontrolled execution)"
                        );
                    }
                    ResultAction::Pipeline {
                        ref path,
                        ref prompt,
                    } => {
                        let entry = execute_sub_pipeline(
                            path,
                            prompt.as_deref(),
                            &step_id,
                            session,
                            runner,
                            depth,
                            pipeline_base_dir,
                        )?;
                        session.turn_log.append(entry);
                    }
                }
            }
        }
    }

    Ok(ExecuteOutcome::Completed)
}

/// Execute all steps in `session.pipeline` in order.
///
/// SPEC §4.2 core invariant: once execution begins, all steps run in order.
/// Early exit only via explicit declared outcomes — never silent failures.
pub fn execute(session: &mut Session, runner: &dyn Runner) -> Result<ExecuteOutcome, AilError> {
    execute_inner(session, runner, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{
        ActionKind, Condition, ContextSource, ExitCodeMatch, Pipeline, ResultAction, ResultBranch,
        ResultMatcher, Step, StepBody, StepId,
    };
    use crate::error::error_types;
    use crate::runner::stub::StubRunner;
    use crate::session::{NullProvider, Session};

    fn make_pipeline(steps: Vec<Step>) -> Pipeline {
        Pipeline {
            steps,
            source: None,
            defaults: Default::default(),
            timeout_seconds: None,
            default_tools: None,
        }
    }

    fn make_session(steps: Vec<Step>) -> Session {
        Session::new(make_pipeline(steps), "test prompt".to_string())
            .with_log_provider(Box::new(NullProvider))
    }

    fn prompt_step(id: &str, text: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt(text.to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    fn prompt_step_with_on_result(id: &str, branches: Vec<ResultBranch>) -> Step {
        Step {
            on_result: Some(branches),
            ..prompt_step(id, "test prompt")
        }
    }

    fn context_shell_step(id: &str, cmd: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Context(ContextSource::Shell(cmd.to_string())),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    fn context_shell_step_with_on_result(id: &str, cmd: &str, branches: Vec<ResultBranch>) -> Step {
        Step {
            on_result: Some(branches),
            ..context_shell_step(id, cmd)
        }
    }

    fn action_step(id: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Action(ActionKind::PauseForHuman),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    fn skill_step(id: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Skill(std::path::PathBuf::from("some-skill")),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        }
    }

    /// SPEC §12 — condition: never causes the step to be skipped entirely.
    #[test]
    fn condition_never_skips_step() {
        let mut skipped = prompt_step("skipped", "this should not run");
        skipped.condition = Some(Condition::Never);

        let after = prompt_step("after", "this should run");

        let mut session = make_session(vec![skipped, after]);
        let runner = StubRunner::new("ok");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1, "only the 'after' step should have run");
        assert_eq!(entries[0].step_id, "after");
    }

    /// SPEC §12 — condition: never skips all steps when every step is never.
    #[test]
    fn condition_never_on_all_steps_produces_empty_turn_log() {
        let mut s1 = prompt_step("s1", "text");
        s1.condition = Some(Condition::Never);
        let mut s2 = prompt_step("s2", "text");
        s2.condition = Some(Condition::Never);

        let mut session = make_session(vec![s1, s2]);
        let runner = StubRunner::new("never called");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            session.turn_log.entries().is_empty(),
            "no entries expected when all steps are skipped"
        );
    }

    /// SPEC §5.3 — context:shell: step populates stdout, stderr, and exit_code in turn log.
    #[test]
    fn context_shell_step_populates_stdout_stderr_exit_code() {
        let step = context_shell_step("ctx", "echo hello_out; echo err_out >&2; exit 0");
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.step_id, "ctx");
        assert!(
            entry.stdout.as_deref().unwrap_or("").contains("hello_out"),
            "stdout should contain 'hello_out'"
        );
        assert_eq!(entry.exit_code, Some(0));
        assert!(
            entry.response.is_none(),
            "context steps have no response field"
        );
    }

    /// SPEC §5.3 — context:shell: step with non-zero exit code captures exit_code correctly.
    #[test]
    fn context_shell_step_captures_nonzero_exit_code() {
        let step = context_shell_step("ctx", "exit 42");
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(
            result.is_ok(),
            "non-zero exit code is a result, not an error"
        );
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].exit_code, Some(42));
    }

    /// SPEC §5.4 — on_result contains: matcher that matches → action fires (continue).
    #[test]
    fn on_result_contains_match_action_fires() {
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Contains("needle".to_string()),
                action: ResultAction::Continue,
            }],
        );
        let after = prompt_step("after", "next");
        let mut session = make_session(vec![step, after]);
        // Runner returns a response containing the needle.
        let runner = StubRunner::new("response with needle in it");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 2, "both steps should have run");
    }

    /// SPEC §5.4 — on_result contains: matcher that doesn't match → no action fires, pipeline continues.
    #[test]
    fn on_result_contains_no_match_no_action_fires() {
        // Branch: if response contains "needle" → break. But response won't contain needle.
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Contains("needle".to_string()),
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("after", "next");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("no match here");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(
            entries.len(),
            2,
            "both steps should run because contains: did not match"
        );
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Completed),
            "should complete, not break"
        );
    }

    /// SPEC §5.4 — on_result contains: matching is case-insensitive.
    #[test]
    fn on_result_contains_match_is_case_insensitive() {
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Contains("SUCCESS".to_string()),
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("unreachable", "never");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("task ended with success status");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Break { .. }),
            "case-insensitive match should fire break"
        );
        assert_eq!(session.turn_log.entries().len(), 1);
    }

    /// SPEC §5.4 — on_result: break returns Ok(Break { step_id }) with the correct step id.
    #[test]
    fn on_result_break_returns_correct_step_id() {
        let step = prompt_step_with_on_result(
            "breaking_step",
            vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("unreachable", "never");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("any response");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        match result.unwrap() {
            ExecuteOutcome::Break { step_id } => {
                assert_eq!(step_id, "breaking_step");
            }
            other => panic!("expected Break, got {other:?}"),
        }
        assert_eq!(session.turn_log.entries().len(), 1);
    }

    /// SPEC §5.4 — on_result: abort_pipeline returns Err with PIPELINE_ABORTED error_type.
    #[test]
    fn on_result_abort_pipeline_returns_pipeline_aborted_error() {
        let step = prompt_step_with_on_result(
            "aborter",
            vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::AbortPipeline,
            }],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("any response");
        let result = execute(&mut session, &runner);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type,
            error_types::PIPELINE_ABORTED,
            "on_result abort_pipeline must return PIPELINE_ABORTED"
        );
        assert!(
            err.detail.contains("aborter"),
            "error detail should reference the step id"
        );
    }

    /// SPEC §5.4 — on_result: exit_code matcher on context:shell: step.
    #[test]
    fn on_result_exit_code_exact_match_on_context_step() {
        let step = context_shell_step_with_on_result(
            "linter",
            "exit 2",
            vec![ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Exact(2)),
                action: ResultAction::Break,
            }],
        );
        let after = prompt_step("unreachable", "never");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Break { .. }),
            "exit_code: 2 should trigger break"
        );
        assert_eq!(session.turn_log.entries().len(), 1);
    }

    /// SPEC §5.4 — on_result: exit_code: any matches non-zero on context step, fires action.
    #[test]
    fn on_result_exit_code_any_matches_nonzero_context_step() {
        let step = context_shell_step_with_on_result(
            "build",
            "exit 1",
            vec![ResultBranch {
                matcher: ResultMatcher::ExitCode(ExitCodeMatch::Any),
                action: ResultAction::AbortPipeline,
            }],
        );
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().error_type,
            error_types::PIPELINE_ABORTED
        );
    }

    /// SPEC §4.2 — pause_for_human action step is a no-op in headless mode; pipeline continues.
    #[test]
    fn pause_for_human_action_step_is_noop_in_headless_mode() {
        let pause = action_step("gate");
        let after = prompt_step("after", "continue");
        let mut session = make_session(vec![pause, after]);
        let runner = StubRunner::new("ok");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Completed),
            "pipeline should complete after no-op pause_for_human"
        );
        // pause_for_human continues without appending a TurnEntry; only 'after' adds an entry.
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1, "pause_for_human produces no turn entry");
        assert_eq!(entries[0].step_id, "after");
    }

    /// SPEC §4.2 — skill: step aborts with PIPELINE_ABORTED (stub in v0.2).
    #[test]
    fn skill_step_aborts_with_pipeline_aborted_error() {
        let step = skill_step("my_skill");
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type,
            error_types::PIPELINE_ABORTED,
            "skill: step must abort with PIPELINE_ABORTED until implemented"
        );
    }

    /// SPEC §4.2 — empty pipeline (no steps) returns Completed without any entries.
    #[test]
    fn empty_pipeline_returns_completed() {
        let mut session = make_session(vec![]);
        let runner = StubRunner::new("unused");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), ExecuteOutcome::Completed),
            "empty pipeline should return Completed"
        );
        assert!(session.turn_log.entries().is_empty());
    }

    /// SPEC §4.2 — multi-step pipeline: all steps run in order and produce entries.
    #[test]
    fn multi_step_pipeline_runs_all_steps_in_order() {
        let s1 = prompt_step("first", "prompt one");
        let s2 = prompt_step("second", "prompt two");
        let s3 = prompt_step("third", "prompt three");
        let mut session = make_session(vec![s1, s2, s3]);
        let runner = StubRunner::new("stub");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].step_id, "first");
        assert_eq!(entries[1].step_id, "second");
        assert_eq!(entries[2].step_id, "third");
    }

    /// Passthrough pipeline (Pipeline::passthrough) runs the invocation step.
    #[test]
    fn passthrough_pipeline_runs_invocation_step() {
        let mut session = Session::new(Pipeline::passthrough(), "hello world".to_string())
            .with_log_provider(Box::new(NullProvider));
        let runner = StubRunner::new("response");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].step_id, "invocation");
    }

    /// SPEC §5.4 — no on_result defined: pipeline always continues to next step.
    #[test]
    fn step_without_on_result_always_continues() {
        let s1 = prompt_step("s1", "step one");
        let s2 = prompt_step("s2", "step two");
        let mut session = make_session(vec![s1, s2]);
        let runner = StubRunner::new("anything");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 2);
    }

    /// SPEC §5.4 — on_result: continue branch fires and pipeline continues to next step.
    #[test]
    fn on_result_continue_branch_pipeline_continues() {
        let step = prompt_step_with_on_result(
            "check",
            vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Continue,
            }],
        );
        let after = prompt_step("after", "next");
        let mut session = make_session(vec![step, after]);
        let runner = StubRunner::new("anything");
        let result = execute(&mut session, &runner);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
        assert_eq!(session.turn_log.entries().len(), 2);
    }
}
