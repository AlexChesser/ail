//! TUI-controlled pipeline execution — `execute_with_control()`.

#![allow(clippy::result_large_err)]

use crate::config::domain::HitlHeadlessBehavior;
use crate::error::AilError;
use crate::runner::{InvokeOptions, RunResult, Runner, RunnerEvent};
use crate::session::Session;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use super::core::{execute_core, BeforeStepAction, StepObserver};
use super::events::{ExecuteOutcome, ExecutionControl, ExecutorEvent};

// ── ChannelObserver ───────────────────────────────────────────────────────────

/// Controlled-mode `StepObserver`: emits [`ExecutorEvent`]s, blocks on HITL gates,
/// and invokes the runner via `invoke_streaming` with a per-step forwarder thread.
struct ChannelObserver<'a> {
    event_tx: mpsc::Sender<ExecutorEvent>,
    hitl_rx: mpsc::Receiver<String>,
    control: &'a ExecutionControl,
    disabled_steps: &'a HashSet<String>,
}

impl<'a> StepObserver for ChannelObserver<'a> {
    fn before_step(
        &mut self,
        step_id: &str,
        _step_index: usize,
        condition_skip: bool,
    ) -> BeforeStepAction {
        // Kill check.
        if self.control.kill_requested.is_cancelled() {
            tracing::info!(step_id = %step_id, "kill requested — stopping pipeline");
            return BeforeStepAction::Stop;
        }

        // Pause spin-wait — exit if kill is set while paused.
        while self.control.pause_requested.load(Ordering::SeqCst) {
            if self.control.kill_requested.is_cancelled() {
                return BeforeStepAction::Stop;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if self.control.kill_requested.is_cancelled() {
            return BeforeStepAction::Stop;
        }

        // Disabled check.
        if self.disabled_steps.contains(step_id) {
            let _ = self.event_tx.send(ExecutorEvent::StepSkipped {
                step_id: step_id.to_string(),
            });
            return BeforeStepAction::Skip;
        }

        // Condition check (evaluated in execute_core before calling this hook).
        if condition_skip {
            tracing::info!(step_id = %step_id, "step skipped by condition");
            let _ = self.event_tx.send(ExecutorEvent::StepSkipped {
                step_id: step_id.to_string(),
            });
            return BeforeStepAction::Skip;
        }

        BeforeStepAction::Run
    }

    fn on_non_prompt_started(&mut self, step_id: &str, step_index: usize, total_steps: usize) {
        let _ = self.event_tx.send(ExecutorEvent::StepStarted {
            step_id: step_id.to_string(),
            step_index,
            total_steps,
            resolved_prompt: None,
        });
    }

    fn on_prompt_ready(
        &mut self,
        step_id: &str,
        step_index: usize,
        total_steps: usize,
        resolved: &str,
    ) {
        let _ = self.event_tx.send(ExecutorEvent::StepStarted {
            step_id: step_id.to_string(),
            step_index,
            total_steps,
            resolved_prompt: Some(resolved.to_string()),
        });
    }

    fn on_step_failed(&mut self, step_id: &str, detail: &str) {
        let _ = self.event_tx.send(ExecutorEvent::StepFailed {
            step_id: step_id.to_string(),
            error: detail.to_string(),
        });
    }

    fn on_step_error_continued(&mut self, step_id: &str, error: &str, error_type: &str) {
        let _ = self.event_tx.send(ExecutorEvent::StepErrorContinued {
            step_id: step_id.to_string(),
            error: error.to_string(),
            error_type: error_type.to_string(),
        });
    }

    fn on_step_retrying(&mut self, step_id: &str, error: &str, attempt: u32, max_retries: u32) {
        let _ = self.event_tx.send(ExecutorEvent::StepRetrying {
            step_id: step_id.to_string(),
            error: error.to_string(),
            attempt,
            max_retries,
        });
    }

    fn augment_options(&self, opts: &mut InvokeOptions) {
        opts.cancel_token = Some(self.control.kill_requested.clone());
        opts.permission_responder = self.control.permission_responder.clone();
    }

    fn invoke(
        &mut self,
        runner: &dyn Runner,
        prompt: &str,
        opts: InvokeOptions,
    ) -> Result<RunResult, AilError> {
        let (runner_tx, runner_rx) = mpsc::channel::<RunnerEvent>();
        let event_tx_clone = self.event_tx.clone();
        let fwd_handle = std::thread::spawn(move || {
            for ev in runner_rx {
                let _ = event_tx_clone.send(ExecutorEvent::RunnerEvent { event: ev });
            }
        });
        let result = runner.invoke_streaming(prompt, opts, runner_tx);
        let _ = fwd_handle.join();
        result
    }

    fn on_prompt_completed(&mut self, step_id: &str, result: &RunResult) {
        let _ = self.event_tx.send(ExecutorEvent::StepCompleted {
            step_id: step_id.to_string(),
            cost_usd: result.cost_usd,
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            response: Some(result.response.clone()),
            model: result.model.clone(),
        });
    }

    fn on_non_prompt_completed(&mut self, step_id: &str) {
        let _ = self.event_tx.send(ExecutorEvent::StepCompleted {
            step_id: step_id.to_string(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            response: None,
            model: None,
        });
    }

    fn handle_pause_for_human(&mut self, step_id: &str, message: Option<&str>) {
        tracing::info!(step_id = %step_id, "pause_for_human — waiting for HITL response");
        let _ = self.event_tx.send(ExecutorEvent::HitlGateReached {
            step_id: step_id.to_string(),
            message: message.map(str::to_string),
        });
        let _response = self.hitl_rx.recv().unwrap_or_default();
        tracing::info!(step_id = %step_id, "HITL gate unblocked — resuming");
        let _ = self.event_tx.send(ExecutorEvent::StepCompleted {
            step_id: step_id.to_string(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            response: None,
            model: None,
        });
    }

    fn handle_modify_output(
        &mut self,
        step_id: &str,
        message: Option<&str>,
        last_response: Option<&str>,
        _headless_behavior: &HitlHeadlessBehavior,
        _default_value: Option<&str>,
    ) -> Result<Option<String>, AilError> {
        tracing::info!(step_id = %step_id, "modify_output gate — waiting for HITL response");
        let _ = self.event_tx.send(ExecutorEvent::HitlModifyReached {
            step_id: step_id.to_string(),
            message: message.map(str::to_string),
            last_response: last_response.map(str::to_string),
        });
        let modified = self.hitl_rx.recv().unwrap_or_default();
        tracing::info!(step_id = %step_id, "modify_output HITL gate unblocked — resuming");
        let _ = self.event_tx.send(ExecutorEvent::StepCompleted {
            step_id: step_id.to_string(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            response: None,
            model: None,
        });
        Ok(Some(modified))
    }

    fn on_pipeline_done(&mut self, outcome: &ExecuteOutcome) {
        let serialisable = match outcome {
            ExecuteOutcome::Completed => ExecuteOutcome::Completed,
            ExecuteOutcome::Break { step_id } => ExecuteOutcome::Break {
                step_id: step_id.clone(),
            },
            ExecuteOutcome::Error(e) => ExecuteOutcome::Error(e.clone()),
        };
        let _ = self
            .event_tx
            .send(ExecutorEvent::PipelineCompleted(serialisable));
    }

    fn on_pipeline_error(&mut self, err: &AilError) {
        let _ = self.event_tx.send(ExecutorEvent::PipelineError {
            error: err.detail().to_owned(),
            error_type: err.error_type().to_string(),
        });
    }

    fn on_result_pause(&mut self, step_id: &str, message: Option<&str>) {
        tracing::info!(step_id = %step_id, "on_result pause_for_human — waiting for HITL response");
        let _ = self.event_tx.send(ExecutorEvent::HitlGateReached {
            step_id: step_id.to_string(),
            message: message.map(str::to_string),
        });
        let _response = self.hitl_rx.recv().unwrap_or_default();
        tracing::info!(step_id = %step_id, "on_result HITL gate unblocked — resuming");
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Execute the pipeline with live control signals and event streaming for the TUI.
///
/// Sends [`ExecutorEvent`]s through `event_tx`; respects kill/pause flags between steps.
/// Steps listed in `disabled_steps` are skipped with a `StepSkipped` event.
/// Blocks on `hitl_rx.recv()` when a `pause_for_human` step is reached.
pub fn execute_with_control(
    session: &mut Session,
    runner: &dyn Runner,
    control: &ExecutionControl,
    disabled_steps: &HashSet<String>,
    event_tx: mpsc::Sender<ExecutorEvent>,
    hitl_rx: mpsc::Receiver<String>,
) -> Result<ExecuteOutcome, AilError> {
    let mut observer = ChannelObserver {
        event_tx,
        hitl_rx,
        control,
        disabled_steps,
    };
    execute_core(session, runner, &mut observer, 0)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;

    use crate::config::domain::{
        ActionKind, Condition, ContextSource, ResultAction, ResultBranch, ResultMatcher, Step,
        StepBody, StepId,
    };
    use crate::executor::events::{ExecuteOutcome, ExecutionControl, ExecutorEvent};
    use crate::runner::stub::StubRunner;
    use crate::runner::CancelToken;
    use crate::test_helpers::{make_session, prompt_step};

    fn make_control() -> ExecutionControl {
        ExecutionControl {
            kill_requested: CancelToken::new(),
            pause_requested: Arc::new(AtomicBool::new(false)),
            permission_responder: None,
        }
    }

    fn collect_events(rx: mpsc::Receiver<ExecutorEvent>) -> Vec<ExecutorEvent> {
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        events
    }

    // ── Test 1: Empty pipeline → PipelineCompleted event sent ──────────────

    #[test]
    fn empty_pipeline_sends_pipeline_completed_event() {
        let mut session = make_session(vec![]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed)
            )),
            "Expected PipelineCompleted event, got: {events:?}"
        );
    }

    // ── Test 2: kill_requested set before call → pipeline stops early ───────

    #[test]
    fn kill_requested_before_call_skips_all_steps() {
        let mut session = make_session(vec![prompt_step("step1", "do something")]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        control.kill_requested.cancel();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        // No step entries recorded — kill fired before step ran.
        assert_eq!(session.turn_log.entries().len(), 0);
        // PipelineCompleted should still be sent (loop breaks then falls through to it).
        let events = collect_events(rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ExecutorEvent::PipelineCompleted(_))),
            "Expected PipelineCompleted event after kill, got: {events:?}"
        );
    }

    // ── Test 3: Condition::Never → StepSkipped event ────────────────────────

    #[test]
    fn condition_never_sends_step_skipped_event() {
        let step = Step {
            id: StepId("conditional".to_string()),
            body: StepBody::Prompt("should not run".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: Some(Condition::Never),
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 0);

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepSkipped { step_id } if step_id == "conditional"
            )),
            "Expected StepSkipped event, got: {events:?}"
        );
    }

    // ── Test 4: Single prompt step happy path ────────────────────────────────

    #[test]
    fn single_prompt_step_sends_started_and_completed_events() {
        let mut session = make_session(vec![prompt_step("review", "Please review")]);
        let runner = StubRunner::new("looks good");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "review");

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepStarted { step_id, .. } if step_id == "review"
            )),
            "Expected StepStarted event, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepCompleted { step_id, .. } if step_id == "review"
            )),
            "Expected StepCompleted event, got: {events:?}"
        );
    }

    // ── Test 5: Unresolvable template → StepFailed event ───────────────────

    #[test]
    fn unresolvable_template_sends_step_failed_event() {
        let mut session = make_session(vec![prompt_step("bad", "{{ totally.unknown.var }}")]);
        let runner = StubRunner::new("never called");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_err());
        assert_eq!(session.turn_log.entries().len(), 0);

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepFailed { step_id, .. } if step_id == "bad"
            )),
            "Expected StepFailed event, got: {events:?}"
        );
    }

    // ── Test 6: context:shell step → StepStarted + StepCompleted ────────────

    #[test]
    fn context_shell_step_sends_started_and_completed_events() {
        let step = Step {
            id: StepId("shell_ctx".to_string()),
            body: StepBody::Context(ContextSource::Shell("echo hello".to_string())),
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
            output_schema: None,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "shell_ctx");
        // stdout should contain "hello"
        assert!(session.turn_log.entries()[0]
            .stdout
            .as_deref()
            .unwrap_or("")
            .contains("hello"));

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepStarted { step_id, .. } if step_id == "shell_ctx"
            )),
            "Expected StepStarted event, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepCompleted { step_id, .. } if step_id == "shell_ctx"
            )),
            "Expected StepCompleted event, got: {events:?}"
        );
    }

    // ── Test 7: pause_for_human action step → HitlGateReached then unblocked ─

    #[test]
    fn pause_for_human_sends_hitl_gate_reached_and_completes() {
        let step = Step {
            id: StepId("gate".to_string()),
            body: StepBody::Action(ActionKind::PauseForHuman),
            message: Some("Waiting for approval".to_string()),
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
            output_schema: None,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("stub");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (hitl_tx, hitl_rx) = mpsc::channel::<String>();

        // Send the unblock signal immediately — the main thread will receive it when it blocks.
        hitl_tx.send("approved".to_string()).unwrap();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::HitlGateReached { step_id, .. } if step_id == "gate"
            )),
            "Expected HitlGateReached event, got: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepCompleted { step_id, .. } if step_id == "gate"
            )),
            "Expected StepCompleted after unblocking, got: {events:?}"
        );
    }

    // ── Test 8: on_result: break branch → PipelineCompleted with Break outcome ─

    #[test]
    fn on_result_break_sends_pipeline_completed_with_break_outcome() {
        let step = Step {
            id: StepId("check".to_string()),
            body: StepBody::Prompt("evaluate".to_string()),
            message: None,
            tools: None,
            on_result: Some(vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::Break,
            }]),
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("any response");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExecuteOutcome::Break { step_id } if step_id == "check"));

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::PipelineCompleted(ExecuteOutcome::Break { step_id }) if step_id == "check"
            )),
            "Expected PipelineCompleted(Break) event, got: {events:?}"
        );
    }

    // ── Test 9: Two sequential prompt steps both complete ───────────────────

    #[test]
    fn two_sequential_prompt_steps_both_complete() {
        let mut session = make_session(vec![
            prompt_step("step_a", "First prompt"),
            prompt_step("step_b", "Second prompt"),
        ]);
        let runner = StubRunner::new("ok");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        assert_eq!(session.turn_log.entries().len(), 2);
        assert_eq!(session.turn_log.entries()[0].step_id, "step_a");
        assert_eq!(session.turn_log.entries()[1].step_id, "step_b");

        let events = collect_events(rx);
        let started: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::StepStarted { .. }))
            .collect();
        let completed: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ExecutorEvent::StepCompleted { .. }))
            .collect();
        assert_eq!(started.len(), 2, "Expected 2 StepStarted events");
        assert_eq!(completed.len(), 2, "Expected 2 StepCompleted events");
    }

    // ── Test 10: pause_requested → pipeline pauses then resumes ─────────────

    #[test]
    fn pause_requested_then_cleared_pipeline_completes() {
        // Use two steps so we have a chance to observe pause behaviour between them.
        let mut session = make_session(vec![
            prompt_step("step_a", "First"),
            prompt_step("step_b", "Second"),
        ]);
        let runner = StubRunner::new("ok");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        // Set pause immediately — the executor will spin-wait between steps.
        let pause_flag = Arc::clone(&control.pause_requested);
        let kill_flag = control.kill_requested.clone();

        // Spawn a thread that clears the pause after a short delay.
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            pause_flag.store(false, Ordering::SeqCst);
            // Ensure kill is not set.
            let _ = kill_flag.is_cancelled();
        });

        control.pause_requested.store(true, Ordering::SeqCst);

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        // Both steps should have run after the pause was cleared.
        assert_eq!(session.turn_log.entries().len(), 2);

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed)
            )),
            "Expected PipelineCompleted event after pause+resume"
        );
    }

    // ── Test 11: disabled_steps set → StepSkipped event (not condition check) ─

    #[test]
    fn disabled_step_sends_step_skipped_event() {
        let mut session = make_session(vec![
            prompt_step("enabled", "run me"),
            prompt_step("disabled_step", "skip me"),
        ]);
        let runner = StubRunner::new("ok");
        let control = make_control();
        let mut disabled = HashSet::new();
        disabled.insert("disabled_step".to_string());
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());
        // Only 1 entry: the enabled step.
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "enabled");

        let events = collect_events(rx);
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::StepSkipped { step_id } if step_id == "disabled_step"
            )),
            "Expected StepSkipped for disabled_step, got: {events:?}"
        );
    }

    // ── Test 12: on_result: abort_pipeline → PipelineError event and Err ─────

    #[test]
    fn on_result_abort_pipeline_sends_pipeline_error_event() {
        let step = Step {
            id: StepId("aborter".to_string()),
            body: StepBody::Prompt("evaluate".to_string()),
            message: None,
            tools: None,
            on_result: Some(vec![ResultBranch {
                matcher: ResultMatcher::Always,
                action: ResultAction::AbortPipeline,
            }]),
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
        };
        let mut session = make_session(vec![step]);
        let runner = StubRunner::new("any response");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_err());

        let events = collect_events(rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ExecutorEvent::PipelineError { .. })),
            "Expected PipelineError event, got: {events:?}"
        );
    }

    // ── Test 13: resolved_prompt in StepStarted contains the template value ──

    #[test]
    fn step_started_event_contains_resolved_prompt() {
        let mut session =
            make_session(vec![prompt_step("meta", "Run ID is {{ pipeline.run_id }}")]);
        let run_id = session.run_id.clone();
        let runner = StubRunner::new("ok");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx)
            .unwrap();

        let events = collect_events(rx);
        let started = events
            .iter()
            .find(|e| matches!(e, ExecutorEvent::StepStarted { step_id, .. } if step_id == "meta"));
        assert!(started.is_some(), "Expected StepStarted for 'meta'");
        if let Some(ExecutorEvent::StepStarted {
            resolved_prompt: Some(prompt),
            ..
        }) = started
        {
            assert!(
                prompt.contains(&run_id),
                "Expected run_id in resolved_prompt, got: {prompt}"
            );
        }
    }

    // ── Test 14: modify_output gate in controlled mode ──────────────────────

    #[test]
    fn modify_output_sends_hitl_modify_reached_and_stores_modified_text() {
        use crate::config::domain::{ActionKind, HitlHeadlessBehavior};

        let generate = prompt_step("generate", "Generate content");
        let gate = Step {
            id: StepId("review_gate".to_string()),
            body: StepBody::Action(ActionKind::ModifyOutput {
                headless_behavior: HitlHeadlessBehavior::Skip,
                default_value: None,
            }),
            message: Some("Review and edit the output".to_string()),
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
            output_schema: None,
        };

        let mut session = make_session(vec![generate, gate]);
        let runner = StubRunner::new("generated content");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (hitl_tx, hitl_rx) = mpsc::channel::<String>();

        // Simulate human editing the output.
        hitl_tx.send("human-edited content".to_string()).unwrap();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());

        let events = collect_events(rx);
        // Should have HitlModifyReached event
        assert!(
            events.iter().any(|e| matches!(
                e,
                ExecutorEvent::HitlModifyReached { step_id, .. } if step_id == "review_gate"
            )),
            "Expected HitlModifyReached event, got: {events:?}"
        );

        // The modified text should be stored in the turn log.
        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 2); // generate + review_gate
        assert_eq!(entries[1].step_id, "review_gate");
        assert_eq!(entries[1].modified.as_deref(), Some("human-edited content"));
    }

    // ── Test 15: HitlModifyReached event includes last_response ─────────────

    #[test]
    fn modify_output_event_includes_last_response() {
        use crate::config::domain::{ActionKind, HitlHeadlessBehavior};

        let generate = prompt_step("gen", "Generate");
        let gate = Step {
            id: StepId("gate".to_string()),
            body: StepBody::Action(ActionKind::ModifyOutput {
                headless_behavior: HitlHeadlessBehavior::Skip,
                default_value: None,
            }),
            message: Some("Edit this".to_string()),
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
            output_schema: None,
        };

        let mut session = make_session(vec![generate, gate]);
        let runner = StubRunner::new("original output");
        let control = make_control();
        let disabled = HashSet::new();
        let (tx, rx) = mpsc::channel::<ExecutorEvent>();
        let (hitl_tx, hitl_rx) = mpsc::channel::<String>();

        hitl_tx.send("edited".to_string()).unwrap();

        let result =
            super::execute_with_control(&mut session, &runner, &control, &disabled, tx, hitl_rx);

        assert!(result.is_ok());

        let events = collect_events(rx);
        let modify_event = events.iter().find(|e| {
            matches!(
                e,
                ExecutorEvent::HitlModifyReached { step_id, .. } if step_id == "gate"
            )
        });
        assert!(modify_event.is_some(), "Expected HitlModifyReached");

        if let Some(ExecutorEvent::HitlModifyReached {
            last_response,
            message,
            ..
        }) = modify_event
        {
            assert_eq!(
                last_response.as_deref(),
                Some("original output"),
                "last_response should be the previous step's output"
            );
            assert_eq!(message.as_deref(), Some("Edit this"));
        }
    }
}
