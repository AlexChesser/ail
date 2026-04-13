//! Shared execution core — the single step-dispatch loop used by both headless and
//! controlled execution modes.
//!
//! The [`StepObserver`] trait is the seam: headless mode uses [`NullObserver`] (all
//! hooks are no-ops); controlled mode uses `ChannelObserver` which emits
//! [`ExecutorEvent`]s and blocks on HITL gates.

#![allow(clippy::result_large_err)]

use crate::config::domain::{
    ActionKind, Condition, ConditionExpr, ContextSource, OnError, ResultAction, Step, StepBody,
    StepId, MAX_LOOP_DEPTH,
};
use crate::error::AilError;
use crate::runner::{InvokeOptions, RunResult, Runner};
use crate::session::turn_log::TurnEntry;
use crate::session::{DoWhileContext, Session};

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

// ── Chain step execution ─────────────────────────────────────────────────────

/// Execute a single step (dispatching by body type), including its own nested
/// before/then chains. This is the recursive building block used by
/// `execute_chain_steps` for both `before:` and `then:` chains, and also by
/// the main loop for top-level steps.
///
/// Returns `Some(ResultAction)` if the step's on_result matched and the caller
/// should handle that action, or `None` if no on_result matched / no on_result defined.
fn execute_single_step<O: StepObserver>(
    step: &Step,
    session: &mut Session,
    runner: &dyn Runner,
    observer: &mut O,
    depth: usize,
    total_steps: usize,
    step_index: usize,
) -> Result<Option<ResultAction>, AilError> {
    let step_id = step.id.as_str().to_string();

    let pipeline_base_dir_buf: Option<std::path::PathBuf> = session
        .pipeline
        .source
        .as_deref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());
    let pipeline_base_dir = pipeline_base_dir_buf.as_deref();

    // Run before: chain steps first (SPEC §5.10).
    execute_chain_steps(&step.before, session, runner, observer, depth)?;

    // Dispatch the main step body.
    if let StepBody::Action(ActionKind::PauseForHuman) = &step.body {
        observer.handle_pause_for_human(&step_id, step.message.as_deref());
        // pause_for_human produces no TurnEntry; skip on_result and then: chain.
        return Ok(None);
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
        return Ok(None);
    }

    // Non-Prompt/Skill steps emit StepStarted before dispatch; Prompt and Skill steps
    // emit after template resolution (they call observer.on_prompt_ready internally).
    if !matches!(step.body, StepBody::Prompt(_) | StepBody::Skill { .. }) {
        observer.on_non_prompt_started(&step_id, step_index, total_steps);
    }

    // Resolve the effective on_error strategy for this step.
    // None → default behaviour (abort).
    let on_error = step.on_error.as_ref().unwrap_or(&OnError::AbortPipeline);

    // Validate input_schema against the preceding step's output, if declared (SPEC §26.2).
    // Capture the validated input JSON for use by field:equals: in on_result.
    let validated_input = if let Some(ref schema) = step.input_schema {
        match validate_input_schema(session, schema, &step_id) {
            Ok(json) => Some(json),
            Err(e) => {
                // Input schema validation failure is a step error — escalate via on_error.
                match on_error {
                    OnError::Continue => {
                        tracing::warn!(
                            run_id = %session.run_id,
                            step_id = %step_id,
                            error_type = e.error_type(),
                            error = %e.detail(),
                            "input_schema validation failed — on_error: continue"
                        );
                        session.turn_log.record_step_error(
                            &step_id,
                            e.error_type(),
                            e.detail(),
                            "continue",
                            None,
                            None,
                        );
                        observer.on_step_error_continued(&step_id, e.detail(), e.error_type());
                        // Run then: chain even when skipping.
                        execute_chain_steps(&step.then, session, runner, observer, depth)?;
                        return Ok(None);
                    }
                    _ => {
                        observer.on_step_failed(&step_id, e.detail());
                        return Err(e);
                    }
                }
            }
        }
    } else {
        None
    };

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
                unreachable!("PauseForHuman handled above")
            }

            StepBody::Action(ActionKind::ModifyOutput { .. }) => {
                unreachable!("ModifyOutput handled above")
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

            StepBody::NamedPipeline { name, prompt } => dispatch::sub_pipeline::execute_named(
                name,
                prompt.as_deref(),
                &step_id,
                session,
                runner,
                depth,
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

            StepBody::DoWhile {
                max_iterations,
                exit_when,
                steps: inner_steps,
            } => execute_do_while(
                &step_id,
                *max_iterations,
                exit_when,
                inner_steps,
                session,
                runner,
                observer,
                depth,
            ),
        };

        match result {
            Ok(entry) => {
                // Validate output_schema if declared (SPEC §26).
                if let Some(ref schema) = step.output_schema {
                    if let Err(e) = validate_output_schema(&entry, schema, &step_id) {
                        // Treat schema validation failure as a step error —
                        // it flows through on_error handling (retry/continue/abort).
                        match on_error {
                            OnError::Continue => {
                                tracing::warn!(
                                    run_id = %session.run_id,
                                    step_id = %step_id,
                                    error_type = e.error_type(),
                                    error = %e.detail(),
                                    "output_schema validation failed — on_error: continue"
                                );
                                session.turn_log.record_step_error(
                                    &step_id,
                                    e.error_type(),
                                    e.detail(),
                                    "continue",
                                    None,
                                    None,
                                );
                                observer.on_step_error_continued(
                                    &step_id,
                                    e.detail(),
                                    e.error_type(),
                                );
                                break;
                            }
                            OnError::Retry { max_retries } if attempt < max_attempts => {
                                tracing::warn!(
                                    run_id = %session.run_id,
                                    step_id = %step_id,
                                    attempt,
                                    error = %e.detail(),
                                    "output_schema validation failed — retrying"
                                );
                                session.turn_log.record_step_error(
                                    &step_id,
                                    e.error_type(),
                                    e.detail(),
                                    "retry",
                                    Some(attempt),
                                    Some(*max_retries),
                                );
                                observer.on_step_retrying(
                                    &step_id,
                                    e.detail(),
                                    attempt,
                                    *max_retries,
                                );
                                continue;
                            }
                            _ => {
                                observer.on_step_failed(&step_id, e.detail());
                                return Err(e);
                            }
                        }
                    }
                }
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
                        observer.on_step_error_continued(&step_id, err.detail(), err.error_type());
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

    // entry_opt is None when on_error: continue swallowed the error — return Ok(None) to skip on_result.
    let Some(entry) = entry_opt else {
        return Ok(None);
    };

    session.turn_log.append(entry);

    // Evaluate on_result branches after step completes.
    let mut matched_action = None;
    if let Some(branches) = &step.on_result {
        let last_entry = session.turn_log.entries().last().expect("just appended");
        if let Some(action) = evaluate_on_result(branches, last_entry, validated_input.as_ref()) {
            matched_action = Some(action.clone());
        }
    }

    // Run then: chain steps after the parent step (and on_result evaluation) (SPEC §5.7).
    execute_chain_steps(&step.then, session, runner, observer, depth)?;

    Ok(matched_action)
}

/// Execute a list of chain steps (before or then). Each chain step may itself
/// have nested before/then chains, handled recursively via `execute_single_step`.
fn execute_chain_steps<O: StepObserver>(
    chain: &[Step],
    session: &mut Session,
    runner: &dyn Runner,
    observer: &mut O,
    depth: usize,
) -> Result<(), AilError> {
    for (idx, chain_step) in chain.iter().enumerate() {
        let step_id = chain_step.id.as_str();
        tracing::info!(
            run_id = %session.run_id,
            step_id = %step_id,
            "executing chain step"
        );
        // Chain steps: total_steps and step_index are not meaningful in the
        // parent pipeline context, so use chain-local values.
        let _action = execute_single_step(
            chain_step,
            session,
            runner,
            observer,
            depth,
            chain.len(),
            idx,
        )?;
        // Chain step on_result actions are consumed locally — they do not
        // propagate break/abort to the parent pipeline. Per spec §5.7:
        // "Use top-level steps if branching is needed."
    }
    Ok(())
}

// ── do_while execution (SPEC §27) ────────────────────────────────────────────

/// Exit reason for a do_while loop, used for logging and result reporting.
enum DoWhileExitReason {
    /// `exit_when` evaluated to true.
    ExitWhen,
    /// A `break` action fired inside the loop body.
    Break,
    /// The iteration budget was exhausted.
    MaxIterations,
}

/// Execute a `do_while:` loop body (SPEC §27).
///
/// Runs `inner_steps` repeatedly until `exit_when` evaluates to true or
/// `max_iterations` is reached. Each iteration executes all inner steps in
/// order; the `exit_when` condition is checked after each complete iteration
/// (post-iteration evaluation, like a do-while loop).
///
/// Inner step IDs are namespaced as `<loop_id>::<step_id>` so they don't
/// collide with outer step IDs. The do_while context is set on the session
/// so template variables `{{ do_while.iteration }}` and
/// `{{ do_while.max_iterations }}` resolve correctly.
///
/// Returns a summary `TurnEntry` for the do_while step itself. The response
/// is the last inner step's response from the final iteration.
#[allow(clippy::too_many_arguments)]
fn execute_do_while<O: StepObserver>(
    loop_step_id: &str,
    max_iterations: u64,
    exit_when: &ConditionExpr,
    inner_steps: &[Step],
    session: &mut Session,
    runner: &dyn Runner,
    observer: &mut O,
    depth: usize,
) -> Result<TurnEntry, AilError> {
    // Depth guard (SPEC §27.9).
    if session.loop_depth >= MAX_LOOP_DEPTH {
        return Err(AilError::LoopDepthExceeded {
            detail: format!(
                "Step '{loop_step_id}' would exceed the maximum loop nesting depth \
                 of {MAX_LOOP_DEPTH}"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                loop_step_id,
            )),
        });
    }

    session.turn_log.record_step_started(
        loop_step_id,
        &format!("do_while(max_iterations={max_iterations})"),
    );

    let result = execute_do_while_inner(
        loop_step_id,
        max_iterations,
        exit_when,
        inner_steps,
        session,
        runner,
        observer,
        depth,
    );

    match &result {
        Ok(_) => observer.on_non_prompt_completed(loop_step_id),
        Err(e) => observer.on_step_failed(loop_step_id, e.detail()),
    }
    result
}

/// Inner loop logic, separated so the caller can attach observer hooks around it.
#[allow(clippy::too_many_arguments)]
fn execute_do_while_inner<O: StepObserver>(
    loop_step_id: &str,
    max_iterations: u64,
    exit_when: &ConditionExpr,
    inner_steps: &[Step],
    session: &mut Session,
    runner: &dyn Runner,
    observer: &mut O,
    depth: usize,
) -> Result<TurnEntry, AilError> {
    // Save and restore outer do_while context for nested loops.
    let prev_context = session.do_while_context.take();
    session.loop_depth += 1;

    let prefix = format!("{loop_step_id}::");
    let total_inner = inner_steps.len();
    let mut index: u64 = 0;
    let mut exit_reason = DoWhileExitReason::MaxIterations;

    for iteration in 0..max_iterations {
        // Clear previous iteration's inner step entries (SPEC §27.3 — iteration scope).
        session.turn_log.remove_entries_with_prefix(&prefix);

        // Set loop context for template variable resolution.
        session.do_while_context = Some(DoWhileContext {
            loop_id: loop_step_id.to_string(),
            iteration,
            max_iterations,
        });

        tracing::info!(
            run_id = %session.run_id,
            step_id = %loop_step_id,
            iteration,
            max_iterations,
            "do_while iteration started"
        );

        // Execute each inner step with a namespaced ID.
        let mut loop_broken = false;
        for (inner_idx, inner_step) in inner_steps.iter().enumerate() {
            let namespaced_id = format!("{}{}", prefix, inner_step.id.as_str());
            let mut namespaced_step = inner_step.clone();
            namespaced_step.id = StepId(namespaced_id.clone());

            // Evaluate condition for the inner step.
            let condition_skip = if let Some(ref cond) = namespaced_step.condition {
                !evaluate_condition(cond, session, &namespaced_id)?
            } else {
                false
            };

            match observer.before_step(&namespaced_id, inner_idx, condition_skip) {
                BeforeStepAction::Run => {}
                BeforeStepAction::Skip => continue,
                BeforeStepAction::Stop => {
                    loop_broken = true;
                    break;
                }
            }

            tracing::info!(
                run_id = %session.run_id,
                step_id = %namespaced_id,
                iteration,
                "executing do_while inner step"
            );

            let matched_action = execute_single_step(
                &namespaced_step,
                session,
                runner,
                observer,
                depth,
                total_inner,
                inner_idx,
            )?;

            // Handle on_result actions within the loop (SPEC §27.3 point 5).
            // `break` exits the loop, not the pipeline.
            if let Some(action) = matched_action {
                match action {
                    ResultAction::Continue => {}
                    ResultAction::Break => {
                        tracing::info!(
                            run_id = %session.run_id,
                            step_id = %loop_step_id,
                            inner_step = %namespaced_id,
                            iteration,
                            "on_result break inside do_while — exiting loop"
                        );
                        loop_broken = true;
                        break;
                    }
                    ResultAction::AbortPipeline => {
                        // Restore state before propagating.
                        session.loop_depth -= 1;
                        session.do_while_context = prev_context;
                        let err = AilError::PipelineAborted {
                            detail: format!(
                                "Step '{namespaced_id}' on_result fired abort_pipeline \
                                 inside do_while loop '{loop_step_id}'"
                            ),
                            context: Some(crate::error::ErrorContext::for_step(
                                &session.run_id,
                                loop_step_id,
                            )),
                        };
                        observer.on_pipeline_error(&err);
                        return Err(err);
                    }
                    ResultAction::PauseForHuman => {
                        observer.on_result_pause(&namespaced_id, None);
                    }
                    ResultAction::Pipeline {
                        ref path,
                        ref prompt,
                    } => {
                        let pipeline_base_dir_buf: Option<std::path::PathBuf> = session
                            .pipeline
                            .source
                            .as_deref()
                            .and_then(|p| p.parent())
                            .map(|p| p.to_path_buf());
                        let pipeline_base_dir = pipeline_base_dir_buf.as_deref();
                        let on_result_step_id = format!("{namespaced_id}__on_result");
                        let sub_entry = dispatch::sub_pipeline::execute_sub_pipeline(
                            path,
                            prompt.as_deref(),
                            &on_result_step_id,
                            session,
                            runner,
                            depth,
                            pipeline_base_dir,
                        )
                        .inspect_err(|e| {
                            // Restore state before propagating.
                            observer.on_pipeline_error(e)
                        })?;
                        session.turn_log.append(sub_entry);
                    }
                }
            }
        }

        // Inner steps completed — count this iteration.
        index += 1;

        if loop_broken {
            exit_reason = DoWhileExitReason::Break;
            break;
        }

        // Post-iteration: evaluate exit_when (SPEC §27.3 point 1).
        let exit_condition = Condition::Expression(exit_when.clone());
        let should_exit = evaluate_condition(&exit_condition, session, loop_step_id)?;

        tracing::info!(
            run_id = %session.run_id,
            step_id = %loop_step_id,
            iteration,
            exit_when_result = should_exit,
            "do_while exit_when evaluated"
        );

        if should_exit {
            exit_reason = DoWhileExitReason::ExitWhen;
            break;
        }
    }

    // Restore state.
    session.loop_depth -= 1;
    session.do_while_context = prev_context;

    tracing::info!(
        run_id = %session.run_id,
        step_id = %loop_step_id,
        index,
        exit_reason = match exit_reason {
            DoWhileExitReason::ExitWhen => "exit_when",
            DoWhileExitReason::Break => "break",
            DoWhileExitReason::MaxIterations => "max_iterations",
        },
        "do_while completed"
    );

    // If max_iterations was exhausted without exit_when becoming true, abort (default).
    if matches!(exit_reason, DoWhileExitReason::MaxIterations) {
        return Err(AilError::DoWhileMaxIterations {
            detail: format!(
                "Step '{loop_step_id}' exhausted do_while.max_iterations ({max_iterations}) \
                 without exit_when becoming true"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                loop_step_id,
            )),
        });
    }

    // Build summary TurnEntry. Response is the last inner step's response from
    // the final iteration.
    let response = session
        .turn_log
        .entries()
        .iter()
        .rev()
        .filter(|e| e.step_id.starts_with(&prefix))
        .find_map(|e| e.response.as_deref())
        .map(|s| s.to_string());

    Ok(TurnEntry {
        step_id: loop_step_id.to_string(),
        prompt: format!("do_while(max_iterations={max_iterations})"),
        response,
        index: Some(index),
        ..Default::default()
    })
}

// ── Output schema validation (SPEC §26) ─────────────────────────────────────

/// Validate a step's response against its declared `output_schema`.
///
/// Parses the response as JSON and validates against the JSON Schema.
/// Returns `Ok(())` if valid, or an `OutputSchemaValidationFailed` error
/// with details about what failed.
fn validate_output_schema(
    entry: &TurnEntry,
    schema: &serde_json::Value,
    step_id: &str,
) -> Result<(), AilError> {
    let response = entry.response.as_deref().unwrap_or("");

    // Parse response as JSON.
    let json_value: serde_json::Value =
        serde_json::from_str(response).map_err(|e| AilError::OutputSchemaValidationFailed {
            detail: format!(
                "Step '{step_id}' declares output_schema but the response is not valid JSON: {e}"
            ),
            context: None,
        })?;

    // Validate against the schema.
    let validator =
        jsonschema::validator_for(schema).map_err(|e| AilError::OutputSchemaValidationFailed {
            detail: format!("Step '{step_id}' output_schema failed to compile as JSON Schema: {e}"),
            context: None,
        })?;

    if let Err(error) = validator.validate(&json_value) {
        return Err(AilError::OutputSchemaValidationFailed {
            detail: format!("Step '{step_id}' output failed output_schema validation: {error}"),
            context: None,
        });
    }

    Ok(())
}

// ── Input schema validation (SPEC §26.2) ────────────────────────────────────

/// Validate the preceding step's output against this step's declared `input_schema`.
///
/// Parses the session's `last_response` as JSON and validates against the schema.
/// Returns the parsed JSON value on success (for use by `field:` + `equals:` in `on_result`),
/// or an `InputSchemaValidationFailed` error with details.
fn validate_input_schema(
    session: &Session,
    schema: &serde_json::Value,
    step_id: &str,
) -> Result<serde_json::Value, AilError> {
    let input = session.turn_log.last_response().unwrap_or("");

    let json_value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| AilError::InputSchemaValidationFailed {
            detail: format!(
                "Step '{step_id}' declares input_schema but the preceding step's output \
                 is not valid JSON: {e}"
            ),
            context: None,
        })?;

    let validator =
        jsonschema::validator_for(schema).map_err(|e| AilError::InputSchemaValidationFailed {
            detail: format!("Step '{step_id}' input_schema failed to compile as JSON Schema: {e}"),
            context: None,
        })?;

    if let Err(error) = validator.validate(&json_value) {
        return Err(AilError::InputSchemaValidationFailed {
            detail: format!(
                "Step '{step_id}' preceding step output failed input_schema validation: {error}"
            ),
            context: None,
        });
    }

    Ok(json_value)
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

        // Execute the step (including before/then chains) and get on_result action.
        let matched_action = execute_single_step(
            step,
            session,
            runner,
            observer,
            depth,
            total_steps,
            step_index,
        )?;

        // Handle on_result action at the top-level pipeline level.
        if let Some(action) = matched_action {
            let pipeline_base_dir_buf: Option<std::path::PathBuf> = session
                .pipeline
                .source
                .as_deref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf());
            let pipeline_base_dir = pipeline_base_dir_buf.as_deref();

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

    let outcome = ExecuteOutcome::Completed;
    observer.on_pipeline_done(&outcome);
    Ok(outcome)
}
