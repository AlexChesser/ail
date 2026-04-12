//! Shared execution core — the single step-dispatch loop used by both headless and
//! controlled execution modes.
//!
//! The [`StepObserver`] trait is the seam: headless mode uses [`NullObserver`] (all
//! hooks are no-ops); controlled mode uses `ChannelObserver` which emits
//! [`ExecutorEvent`]s and blocks on HITL gates.

#![allow(clippy::result_large_err)]

use std::time::SystemTime;

use crate::config::domain::{
    ActionKind, Condition, ContextSource, ResultAction, StepBody, MAX_SUB_PIPELINE_DEPTH,
};
use crate::error::AilError;
use crate::runner::{InvokeOptions, RunResult, Runner};
use crate::session::{Session, TurnEntry};
use crate::template;

use super::events::ExecuteOutcome;
use super::helpers::{
    build_step_runner_box, build_tool_policy, evaluate_on_result, resolve_effective_runner_name,
    resolve_prompt_file, resolve_step_provider, resolve_step_system_prompts, run_shell_command,
};

// ── Observer trait ────────────────────────────────────────────────────────────

/// Whether to proceed with a step, skip it (no entry), or stop the loop.
pub(super) enum BeforeStepAction {
    Run,
    Skip,
    Stop,
}

/// Hook interface separating headless and controlled execution modes.
///
/// - [`NullObserver`]: no-ops, used by headless mode.
/// - `ChannelObserver` in `controlled.rs`: emits [`ExecutorEvent`]s, blocks on HITL gates.
pub(super) trait StepObserver {
    /// Pre-step guard: kill/pause/disabled/condition checks.
    /// Returns `Stop` (break), `Skip` (continue), or `Run` (proceed).
    fn before_step(
        &mut self,
        step_id: &str,
        step_index: usize,
        condition_never: bool,
    ) -> BeforeStepAction;

    /// Called before a non-Prompt step is dispatched. Controlled: emit `StepStarted` with
    /// `resolved_prompt: None`. Headless: no-op.
    fn on_non_prompt_started(&mut self, step_id: &str, step_index: usize, total_steps: usize);

    /// Called after a Prompt step's template is resolved. Controlled: emit `StepStarted`
    /// with `resolved_prompt: Some(resolved)`. Headless: no-op.
    fn on_prompt_ready(
        &mut self,
        step_id: &str,
        step_index: usize,
        total_steps: usize,
        resolved: &str,
    );

    /// Called when any step fails (before returning the error).
    /// Controlled: emit `StepFailed`. Headless: no-op.
    fn on_step_failed(&mut self, step_id: &str, detail: &str);

    /// Fill mode-specific fields into the `InvokeOptions` before invoking the runner.
    /// Controlled: sets `cancel_token` and `permission_responder`. Headless: no-op.
    fn augment_options(&self, opts: &mut InvokeOptions);

    /// Invoke the runner. Headless: `runner.invoke()`. Controlled: `runner.invoke_streaming()`
    /// with an event-forwarding thread.
    fn invoke(
        &mut self,
        runner: &dyn Runner,
        prompt: &str,
        opts: InvokeOptions,
    ) -> Result<RunResult, AilError>;

    /// Called when a Prompt step completes successfully.
    /// Controlled: emit `StepCompleted`. Headless: no-op.
    fn on_prompt_completed(&mut self, step_id: &str, result: &RunResult);

    /// Called when a non-Prompt step (context shell / sub-pipeline) completes.
    /// Controlled: emit `StepCompleted`. Headless: no-op.
    fn on_non_prompt_completed(&mut self, step_id: &str);

    /// Handle a `pause_for_human` action step. Controlled: emit `HitlGateReached`, block on
    /// the HITL channel, then emit `StepCompleted`. Headless: no-op (step is skipped, no entry).
    fn handle_pause_for_human(&mut self, step_id: &str, message: Option<&str>);

    /// Pipeline completed (normal or via `break`). Controlled: emit `PipelineCompleted`.
    /// Headless: no-op.
    fn on_pipeline_done(&mut self, outcome: &ExecuteOutcome);

    /// Called before returning a pipeline-terminating error (`abort_pipeline` action or
    /// sub-pipeline failure in `on_result`). Controlled: emit `PipelineError`. Headless: no-op.
    fn on_pipeline_error(&mut self, err: &AilError);

    /// Called when `on_result: pause_for_human` fires. Controlled: emit `HitlGateReached`
    /// and block. Headless: log trace only.
    fn on_result_pause(&mut self, step_id: &str, message: Option<&str>);
}

// ── NullObserver (headless mode) ──────────────────────────────────────────────

/// Headless `StepObserver` implementation: all hooks are no-ops.
///
/// The only non-trivial method is `before_step`, which skips `Condition::Never` steps,
/// and `invoke`, which calls `runner.invoke()` directly.
pub(super) struct NullObserver;

impl StepObserver for NullObserver {
    fn before_step(&mut self, step_id: &str, _: usize, condition_never: bool) -> BeforeStepAction {
        if condition_never {
            tracing::info!(step_id = %step_id, "step skipped by condition: never");
            BeforeStepAction::Skip
        } else {
            BeforeStepAction::Run
        }
    }

    fn on_non_prompt_started(&mut self, _: &str, _: usize, _: usize) {}

    fn on_prompt_ready(&mut self, _: &str, _: usize, _: usize, _: &str) {}

    fn on_step_failed(&mut self, _: &str, _: &str) {}

    fn augment_options(&self, _: &mut InvokeOptions) {}

    fn invoke(
        &mut self,
        runner: &dyn Runner,
        prompt: &str,
        opts: InvokeOptions,
    ) -> Result<RunResult, AilError> {
        runner.invoke(prompt, opts)
    }

    fn on_prompt_completed(&mut self, _: &str, _: &RunResult) {}

    fn on_non_prompt_completed(&mut self, _: &str) {}

    fn handle_pause_for_human(&mut self, step_id: &str, _: Option<&str>) {
        tracing::info!(step_id = %step_id, "pause_for_human — no-op in headless mode");
    }

    fn on_pipeline_done(&mut self, _: &ExecuteOutcome) {}

    fn on_pipeline_error(&mut self, _: &AilError) {}

    fn on_result_pause(&mut self, step_id: &str, message: Option<&str>) {
        tracing::warn!(
            step_id = %step_id,
            message = ?message,
            "on_result: pause_for_human fired in headless mode — pipeline continues; \
             use controlled mode (--output-format json) for interactive HITL gates"
        );
    }
}

// ── Sub-pipeline execution ────────────────────────────────────────────────────

/// Load and execute a sub-pipeline, returning a `TurnEntry` for the calling step.
///
/// The `path_template` may contain `{{ variable }}` syntax (SPEC §11); it is resolved
/// against `session` before the file is loaded. The sub-pipeline runs in isolation:
/// a fresh `Session` is created with the parent's `last_response` as its invocation prompt.
/// The child's final step response becomes the returned entry's `response` field.
///
/// `depth` guards against infinite recursion; exceeding `MAX_SUB_PIPELINE_DEPTH` aborts.
///
/// **Note:** sub-pipelines always execute in headless mode, even when the parent pipeline
/// is running in controlled mode. Sub-pipeline streaming events and HITL gates are not
/// propagated to the parent's TUI. This is a known limitation (v0.2).
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
        return Err(AilError::PipelineAborted {
            detail: format!(
                "Step '{step_id}' would exceed the maximum sub-pipeline nesting depth \
                 of {MAX_SUB_PIPELINE_DEPTH}"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                step_id,
            )),
        });
    }

    // Resolve template variables in the path (SPEC §11).
    let resolved_path = template::resolve(path_template, session)
        .map_err(|e| e.with_step_context(&session.run_id, step_id))?;

    // Resolve ./relative and ../relative paths against the parent pipeline's directory (SPEC §9).
    let path_buf = if let (true, Some(base)) = (
        resolved_path.starts_with("./") || resolved_path.starts_with("../"),
        base_dir,
    ) {
        base.join(&resolved_path)
    } else {
        std::path::PathBuf::from(&resolved_path)
    };

    let sub_pipeline = crate::config::load(path_buf.as_path())
        .map_err(|e| e.with_step_context(&session.run_id, step_id))?;

    // The sub-pipeline's invocation prompt: use the explicit override when provided
    // (template-resolved against the parent session), otherwise fall back to the
    // parent's most recent response (SPEC §9).
    let invocation_prompt = if let Some(override_template) = prompt_override {
        template::resolve(override_template, session)
            .map_err(|e| e.with_step_context(&session.run_id, step_id))?
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

    execute_core(&mut child_session, runner, &mut NullObserver, depth + 1)?;

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

// ── Core loop ─────────────────────────────────────────────────────────────────

/// Inner execution loop shared by headless and controlled modes.
///
/// The observer receives hook calls at each phase of step execution; its responses
/// determine mode-specific behaviour (event emission, HITL blocking, streaming).
///
/// SPEC §4.2 core invariant: once execution begins, all steps run in order.
/// Early exit only via explicit declared outcomes — never silent failures.
pub(super) fn execute_core<O: StepObserver>(
    session: &mut Session,
    runner: &dyn Runner,
    observer: &mut O,
    depth: usize,
) -> Result<ExecuteOutcome, AilError> {
    if session.pipeline.steps.is_empty() {
        tracing::info!(run_id = %session.run_id, "empty pipeline — no steps to execute");
        let outcome = ExecuteOutcome::Completed;
        observer.on_pipeline_done(&outcome);
        return Ok(outcome);
    }

    let total_steps = session.pipeline.steps.len();

    // Clone to avoid borrow conflict when calling execute_sub_pipeline (&mut session)
    // while iterating step bodies.
    let steps: Vec<_> = session.pipeline.steps.clone();

    for (step_index, step) in steps.iter().enumerate() {
        let step_id = step.id.as_str().to_string();
        let condition_never = step.condition == Some(Condition::Never);

        match observer.before_step(&step_id, step_index, condition_never) {
            BeforeStepAction::Run => {}
            BeforeStepAction::Skip => continue,
            BeforeStepAction::Stop => break,
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

        // pause_for_human is handled before the match — it produces no TurnEntry.
        if let StepBody::Action(ActionKind::PauseForHuman) = &step.body {
            observer.handle_pause_for_human(&step_id, step.message.as_deref());
            continue;
        }

        // Non-Prompt steps emit StepStarted before dispatch; Prompt steps emit after resolution.
        if !matches!(step.body, StepBody::Prompt(_)) {
            observer.on_non_prompt_started(&step_id, step_index, total_steps);
        }

        let entry = match &step.body {
            StepBody::Prompt(template_text) => {
                let template_text = resolve_prompt_file(template_text, &step_id, pipeline_base_dir)
                    .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;
                let resolved = template::resolve(&template_text, session)
                    .map_err(|e| e.with_step_context(&session.run_id, &step_id))
                    .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;

                observer.on_prompt_ready(&step_id, step_index, total_steps, &resolved);

                let resume_id = if step.resume {
                    session
                        .turn_log
                        .last_runner_session_id()
                        .map(|s| s.to_string())
                } else {
                    None
                };
                session.turn_log.record_step_started(&step_id, &resolved);

                let (resolved_system_prompt, resolved_append_system_prompt) =
                    resolve_step_system_prompts(step, session, &step_id, pipeline_base_dir)
                        .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;

                let resolved_provider = resolve_step_provider(session, step);
                let effective_tools = step
                    .tools
                    .as_ref()
                    .or(session.pipeline.default_tools.as_ref());
                // Update session.runner_name so {{ session.tool }} reflects the active runner.
                session.runner_name = resolve_effective_runner_name(step);
                let step_runner_box = build_step_runner_box(
                    step,
                    session.headless,
                    &session.http_session_store,
                    &resolved_provider,
                )
                .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;
                let effective_runner: &dyn Runner = step_runner_box
                    .as_deref()
                    .map(|b| b as &dyn Runner)
                    .unwrap_or(runner);

                let extensions = effective_runner.build_extensions(&resolved_provider);
                let mut options = InvokeOptions {
                    resume_session_id: resume_id,
                    tool_policy: build_tool_policy(effective_tools),
                    model: resolved_provider.model,
                    extensions,
                    permission_responder: None,
                    cancel_token: None,
                    system_prompt: resolved_system_prompt,
                    append_system_prompt: resolved_append_system_prompt,
                };
                observer.augment_options(&mut options);

                let result = observer
                    .invoke(effective_runner, &resolved, options)
                    .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;

                tracing::info!(
                    run_id = %session.run_id,
                    step_id = %step_id,
                    cost_usd = ?result.cost_usd,
                    "step complete"
                );
                observer.on_prompt_completed(&step_id, &result);

                TurnEntry::from_prompt(step_id.clone(), resolved, result)
            }

            StepBody::Context(ContextSource::Shell(cmd)) => {
                session.turn_log.record_step_started(&step_id, cmd);
                let (stdout, stderr, exit_code) = run_shell_command(&session.run_id, &step_id, cmd)
                    .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;
                tracing::info!(
                    run_id = %session.run_id,
                    step_id = %step_id,
                    exit_code,
                    "context shell step complete"
                );
                observer.on_non_prompt_completed(&step_id);
                TurnEntry::from_context(step_id.clone(), cmd.clone(), stdout, stderr, exit_code)
            }

            StepBody::Action(ActionKind::PauseForHuman) => {
                unreachable!("PauseForHuman handled above before the match")
            }

            StepBody::SubPipeline {
                path: path_template,
                prompt,
            } => {
                session
                    .turn_log
                    .record_step_started(&step_id, path_template);
                let entry = execute_sub_pipeline(
                    path_template,
                    prompt.as_deref(),
                    &step_id,
                    session,
                    runner,
                    depth,
                    pipeline_base_dir,
                )
                .inspect_err(|e| observer.on_step_failed(&step_id, e.detail()))?;
                observer.on_non_prompt_completed(&step_id);
                entry
            }

            StepBody::Skill(_) => {
                let err = AilError::PipelineAborted {
                    detail: format!(
                        "Step '{step_id}' uses a step type not yet implemented in v0.1"
                    ),
                    context: Some(crate::error::ErrorContext::for_step(
                        &session.run_id,
                        &step_id,
                    )),
                };
                observer.on_step_failed(&step_id, err.detail());
                return Err(err);
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
                        let outcome = ExecuteOutcome::Break {
                            step_id: step_id.clone(),
                        };
                        observer.on_pipeline_done(&outcome);
                        return Ok(outcome);
                    }
                    ResultAction::AbortPipeline => {
                        let err = AilError::PipelineAborted {
                            detail: format!("Step '{step_id}' on_result fired abort_pipeline"),
                            context: Some(crate::error::ErrorContext::for_step(
                                &session.run_id,
                                &step_id,
                            )),
                        };
                        observer.on_pipeline_error(&err);
                        return Err(err);
                    }
                    ResultAction::PauseForHuman => {
                        observer.on_result_pause(&step_id, step.message.as_deref());
                    }
                    ResultAction::Pipeline {
                        ref path,
                        ref prompt,
                    } => {
                        // Use a derived step ID so the sub-pipeline's response is
                        // addressable as `{{ step.<id>__on_result.response }}` without
                        // shadowing the parent step's own turn log entry (SPEC §11).
                        let on_result_step_id = format!("{step_id}__on_result");
                        let sub_entry = execute_sub_pipeline(
                            path,
                            prompt.as_deref(),
                            &on_result_step_id,
                            session,
                            runner,
                            depth,
                            pipeline_base_dir,
                        )
                        .inspect_err(|e| observer.on_pipeline_error(e))?;
                        session.turn_log.append(sub_entry);
                    }
                }
            }
        }
    }

    let outcome = ExecuteOutcome::Completed;
    observer.on_pipeline_done(&outcome);
    Ok(outcome)
}
