//! Shared execution core — the single step-dispatch loop used by both headless and
//! controlled execution modes.
//!
//! The [`StepObserver`] trait is the seam: headless mode uses [`NullObserver`] (all
//! hooks are no-ops); controlled mode uses `ChannelObserver` which emits
//! [`ExecutorEvent`]s and blocks on HITL gates.

#![allow(clippy::result_large_err)]

use crate::config::domain::{ActionKind, ContextSource, OnError, ResultAction, StepBody};
use crate::error::AilError;
use crate::runner::{InvokeOptions, RunResult, Runner};
use crate::session::turn_log::TurnEntry;
use crate::session::Session;

use super::dispatch;
use super::events::ExecuteOutcome;
use super::helpers::{evaluate_condition, evaluate_on_result};

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
    /// `condition_skip` is `true` when the condition evaluated to `false` (skip step).
    fn before_step(
        &mut self,
        step_id: &str,
        step_index: usize,
        condition_skip: bool,
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

    /// Called when a step fails but `on_error: continue` swallows the error.
    /// Controlled: emit `StepErrorContinued`. Headless: no-op.
    fn on_step_error_continued(&mut self, step_id: &str, error: &str, error_type: &str);

    /// Called when a step fails and is about to be retried.
    /// Controlled: emit `StepRetrying`. Headless: no-op.
    fn on_step_retrying(&mut self, step_id: &str, error: &str, attempt: u32, max_retries: u32);

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

    /// Handle a `modify_output` HITL gate (SPEC §13.2). Controlled: emit `HitlModifyReached`,
    /// block on the HITL channel, return the human's modified text. Headless: behavior depends
    /// on `headless_behavior` (skip/abort/use_default).
    fn handle_modify_output(
        &mut self,
        step_id: &str,
        message: Option<&str>,
        last_response: Option<&str>,
        headless_behavior: &crate::config::domain::HitlHeadlessBehavior,
        default_value: Option<&str>,
    ) -> Result<Option<String>, AilError>;

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
    fn before_step(&mut self, step_id: &str, _: usize, condition_skip: bool) -> BeforeStepAction {
        if condition_skip {
            tracing::info!(step_id = %step_id, "step skipped by condition");
            BeforeStepAction::Skip
        } else {
            BeforeStepAction::Run
        }
    }

    fn on_non_prompt_started(&mut self, _: &str, _: usize, _: usize) {}

    fn on_prompt_ready(&mut self, _: &str, _: usize, _: usize, _: &str) {}

    fn on_step_failed(&mut self, _: &str, _: &str) {}

    fn on_step_error_continued(&mut self, _: &str, _: &str, _: &str) {}

    fn on_step_retrying(&mut self, _: &str, _: &str, _: u32, _: u32) {}

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

    fn handle_modify_output(
        &mut self,
        step_id: &str,
        _message: Option<&str>,
        _last_response: Option<&str>,
        headless_behavior: &crate::config::domain::HitlHeadlessBehavior,
        default_value: Option<&str>,
    ) -> Result<Option<String>, AilError> {
        use crate::config::domain::HitlHeadlessBehavior;
        match headless_behavior {
            HitlHeadlessBehavior::Skip => {
                tracing::warn!(
                    step_id = %step_id,
                    "modify_output gate skipped in headless mode — pipeline continues; \
                     use controlled mode (--output-format json) for interactive HITL gates"
                );
                Ok(None)
            }
            HitlHeadlessBehavior::Abort => {
                tracing::warn!(step_id = %step_id, "modify_output gate fired abort in headless mode");
                Err(AilError::PipelineAborted {
                    detail: format!(
                        "Step '{step_id}' is a modify_output gate with on_headless: abort — \
                         pipeline cannot continue without human input"
                    ),
                    context: None,
                })
            }
            HitlHeadlessBehavior::UseDefault => {
                let value = default_value.unwrap_or("").to_string();
                tracing::info!(
                    step_id = %step_id,
                    default_len = value.len(),
                    "modify_output gate using default_value in headless mode"
                );
                Ok(Some(value))
            }
        }
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

        // Evaluate the condition — `None` means always run.
        let condition_skip = if let Some(ref cond) = step.condition {
            !evaluate_condition(cond, session, &step_id)?
        } else {
            false
        };

        match observer.before_step(&step_id, step_index, condition_skip) {
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

        // modify_output HITL gate — may produce a TurnEntry with modified text, or skip.
        if let StepBody::Action(ActionKind::ModifyOutput {
            ref headless_behavior,
            ref default_value,
        }) = &step.body
        {
            let last_resp = session.turn_log.last_response().map(|s| s.to_string());
            let modified = observer.handle_modify_output(
                &step_id,
                step.message.as_deref(),
                last_resp.as_deref(),
                headless_behavior,
                default_value.as_deref(),
            )?;
            if let Some(modified_text) = modified {
                let msg = step
                    .message
                    .clone()
                    .unwrap_or_else(|| "modify_output".to_string());
                let entry = TurnEntry::from_modify(&step_id, msg, modified_text);
                session.turn_log.append(entry);
            }
            continue;
        }

        // Non-Prompt/Skill steps emit StepStarted before dispatch; Prompt and Skill steps
        // emit after template resolution (they call observer.on_prompt_ready internally).
        if !matches!(step.body, StepBody::Prompt(_) | StepBody::Skill { .. }) {
            observer.on_non_prompt_started(&step_id, step_index, total_steps);
        }

        // Resolve the effective on_error strategy for this step.
        // None → default behaviour (abort).
        let on_error = step.on_error.as_ref().unwrap_or(&OnError::AbortPipeline);

        let max_attempts: u32 = match on_error {
            OnError::Retry { max_retries } => max_retries + 1, // first attempt + retries
            _ => 1,
        };

        let mut entry_opt = None;
        for attempt in 1..=max_attempts {
            let result = match &step.body {
                StepBody::Prompt(template_text) => dispatch::prompt::execute(
                    template_text,
                    step,
                    session,
                    runner,
                    &step_id,
                    step_index,
                    total_steps,
                    pipeline_base_dir,
                    observer,
                ),

                StepBody::Context(ContextSource::Shell(cmd)) => {
                    dispatch::context::execute_shell(cmd, session, &step_id, observer)
                }

                StepBody::Action(ActionKind::PauseForHuman) => {
                    unreachable!("PauseForHuman handled above before the match")
                }

                StepBody::Action(ActionKind::ModifyOutput { .. }) => {
                    unreachable!("ModifyOutput handled above before the match")
                }

                StepBody::SubPipeline {
                    path: path_template,
                    prompt,
                } => dispatch::sub_pipeline::execute(
                    path_template,
                    prompt.as_deref(),
                    &step_id,
                    session,
                    runner,
                    depth,
                    pipeline_base_dir,
                    observer,
                ),

                StepBody::Skill { ref name } => dispatch::skill::execute(
                    name,
                    step,
                    session,
                    runner,
                    &step_id,
                    step_index,
                    total_steps,
                    pipeline_base_dir,
                    observer,
                ),
            };

            match result {
                Ok(entry) => {
                    entry_opt = Some(entry);
                    break;
                }
                Err(err) => {
                    match on_error {
                        OnError::Continue => {
                            tracing::warn!(
                                run_id = %session.run_id,
                                step_id = %step_id,
                                error_type = err.error_type(),
                                error = %err.detail(),
                                "step failed — on_error: continue, proceeding to next step"
                            );
                            session.turn_log.record_step_error(
                                &step_id,
                                err.error_type(),
                                err.detail(),
                                "continue",
                                None,
                                None,
                            );
                            observer.on_step_error_continued(
                                &step_id,
                                err.detail(),
                                err.error_type(),
                            );
                            // No entry produced — skip to next step.
                            break;
                        }
                        OnError::Retry { max_retries } => {
                            if attempt < max_attempts {
                                tracing::warn!(
                                    run_id = %session.run_id,
                                    step_id = %step_id,
                                    attempt,
                                    max_retries = *max_retries,
                                    error_type = err.error_type(),
                                    error = %err.detail(),
                                    "step failed — retrying"
                                );
                                session.turn_log.record_step_error(
                                    &step_id,
                                    err.error_type(),
                                    err.detail(),
                                    "retry",
                                    Some(attempt),
                                    Some(*max_retries),
                                );
                                observer.on_step_retrying(
                                    &step_id,
                                    err.detail(),
                                    attempt,
                                    *max_retries,
                                );
                                // Continue to next loop iteration (retry).
                            } else {
                                // All retries exhausted — abort.
                                tracing::error!(
                                    run_id = %session.run_id,
                                    step_id = %step_id,
                                    max_retries = *max_retries,
                                    error_type = err.error_type(),
                                    error = %err.detail(),
                                    "step failed after all retries exhausted — aborting"
                                );
                                session.turn_log.record_step_error(
                                    &step_id,
                                    err.error_type(),
                                    err.detail(),
                                    "abort_pipeline",
                                    Some(attempt),
                                    Some(*max_retries),
                                );
                                observer.on_pipeline_error(&err);
                                return Err(err);
                            }
                        }
                        OnError::AbortPipeline => {
                            // Default behaviour — propagate error immediately.
                            return Err(err);
                        }
                    }
                }
            }
        }

        // entry_opt is None when on_error: continue swallowed the error.
        let Some(entry) = entry_opt else {
            continue;
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
                        let sub_entry = dispatch::sub_pipeline::execute_sub_pipeline(
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
