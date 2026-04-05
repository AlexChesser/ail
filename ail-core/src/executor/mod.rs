//! Pipeline executor — SPEC §4.2 core invariant.
//!
//! Two execution modes share common step-dispatch helpers:
//! - [`execute`]: headless mode for `--once` and sub-pipeline calls
//! - [`execute_with_control`]: TUI-controlled mode with live event streaming

mod controlled;
mod events;
mod headless;
mod helpers;

pub use controlled::execute_with_control;
pub use events::{ExecuteOutcome, ExecutionControl, ExecutorEvent};
pub use headless::execute;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{Pipeline, Step, StepBody, StepId};
    use crate::runner::stub::StubRunner;
    use crate::runner::RunnerEvent;
    use crate::session::Session;

    fn make_session(steps: Vec<Step>) -> Session {
        let pipeline = Pipeline {
            steps,
            source: None,
            defaults: Default::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        Session::new(pipeline, "invocation prompt".to_string())
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
        }
    }

    #[test]
    fn passthrough_pipeline_runs_invocation_step() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = Session::new(Pipeline::passthrough(), "hello".to_string());
        let runner = StubRunner::new("stub response");
        let result = execute(&mut session, &runner);
        assert!(result.is_ok());
        // passthrough declares invocation as step zero; executor runs it
        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "invocation");

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn single_step_pipeline_appends_to_turn_log() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![prompt_step("review", "Do a review")]);
        let runner = StubRunner::new("looks good");
        execute(&mut session, &runner).unwrap();

        assert_eq!(session.turn_log.entries().len(), 1);
        assert_eq!(session.turn_log.entries()[0].step_id, "review");
        assert_eq!(
            session.turn_log.entries()[0].response.as_deref(),
            Some("looks good")
        );

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn two_step_pipeline_runs_both_steps_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![
            prompt_step("step_a", "First prompt"),
            prompt_step("step_b", "Second prompt"),
        ])
        .with_pipeline("subpipeline");
        let runner = StubRunner::new("stub");
        execute(&mut session, &runner).unwrap();

        let entries = session.turn_log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].step_id, "step_a");
        assert_eq!(entries[1].step_id, "step_b");

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn template_variable_in_prompt_is_resolved_before_runner() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![prompt_step(
            "meta",
            "The run id is: {{ pipeline.run_id }}",
        )]);
        let run_id = session.run_id.clone();
        let runner = StubRunner::new("ok");
        execute(&mut session, &runner).unwrap();

        let prompt_sent = &session.turn_log.entries()[0].prompt;
        assert!(
            prompt_sent.contains(&run_id),
            "Expected run_id in resolved prompt, got: {prompt_sent}"
        );

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn unresolvable_template_aborts_pipeline_with_error() {
        let tmp = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut session = make_session(vec![prompt_step("bad", "{{ totally.unknown.variable }}")]);
        let runner = StubRunner::new("never called");
        let result = execute(&mut session, &runner);
        assert!(result.is_err());
        // No entries should have been appended
        assert_eq!(session.turn_log.entries().len(), 0);

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn executor_event_serializes_step_started() {
        let event = ExecutorEvent::StepStarted {
            step_id: "review".into(),
            step_index: 0,
            total_steps: 3,
            resolved_prompt: Some("Please review the code".into()),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "step_started");
        assert_eq!(json["step_id"], "review");
        assert_eq!(json["step_index"], 0);
        assert_eq!(json["total_steps"], 3);
    }

    #[test]
    fn executor_event_serializes_step_completed() {
        let event = ExecutorEvent::StepCompleted {
            step_id: "review".into(),
            cost_usd: Some(0.003),
            input_tokens: 100,
            output_tokens: 50,
            response: Some("Looks good!".into()),
            model: None,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "step_completed");
        assert_eq!(json["cost_usd"], 0.003);
        assert_eq!(json["input_tokens"], 100);
        assert_eq!(json["output_tokens"], 50);
    }

    #[test]
    fn executor_event_serializes_pipeline_completed() {
        let event = ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed);
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "pipeline_completed");
        assert_eq!(json["outcome"], "completed");
    }

    #[test]
    fn executor_event_serializes_runner_event_with_nested_event_field() {
        let event = ExecutorEvent::RunnerEvent {
            event: RunnerEvent::StreamDelta {
                text: "hello".into(),
            },
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "runner_event");
        assert_eq!(json["event"]["type"], "stream_delta");
        assert_eq!(json["event"]["text"], "hello");
    }

    #[test]
    fn executor_event_serializes_pipeline_error_with_fields() {
        let event = ExecutorEvent::PipelineError {
            error: "something went wrong".into(),
            error_type: "PIPELINE_ABORTED".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(json["type"], "pipeline_error");
        assert_eq!(json["error"], "something went wrong");
        assert_eq!(json["error_type"], "PIPELINE_ABORTED");
    }

    #[test]
    fn execute_outcome_serializes_break() {
        let outcome = ExecuteOutcome::Break {
            step_id: "s1".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&outcome).unwrap()).unwrap();
        assert_eq!(json["outcome"], "break");
        assert_eq!(json["step_id"], "s1");
    }
}
