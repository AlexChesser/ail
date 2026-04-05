use ail_core::config::domain::{Pipeline, Step, StepBody, StepId};
use ail_core::executor::{execute_with_control, ExecuteOutcome, ExecutionControl, ExecutorEvent};
use ail_core::runner::stub::StubRunner;
use ail_core::runner::{PermissionRequest, RunResult, RunnerEvent};
use ail_core::session::Session;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

fn prompt_step(id: &str, text: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
        message: None,
        tools: None,
        model: None,
        on_result: None,
        runner: None,
    }
}

/// SPEC §23 — execute_with_control emits ExecutorEvents that serialize to valid NDJSON.
#[test]
fn events_serialize_to_valid_ndjson() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![
            prompt_step("step_a", "First prompt"),
            prompt_step("step_b", "Second prompt"),
        ],
        defaults: Default::default(),
        source: None,
    };
    let mut session = Session::new(pipeline, "user prompt".to_string());
    let runner = StubRunner::new("stub response");

    let (event_tx, event_rx) = mpsc::channel();
    let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let control = ExecutionControl {
        pause_requested: Arc::new(AtomicBool::new(false)),
        kill_requested: Arc::new(AtomicBool::new(false)),
        permission_responder: None,
    };

    let result = execute_with_control(
        &mut session,
        &runner,
        &control,
        &HashSet::new(),
        event_tx,
        hitl_rx,
    );
    assert!(result.is_ok());

    // Collect all events.
    let events: Vec<ExecutorEvent> = event_rx.iter().collect();

    // Every event must serialize to valid JSON.
    for event in &events {
        let json_str = serde_json::to_string(event)
            .unwrap_or_else(|e| panic!("Failed to serialize event {event:?}: {e}"));
        let _: serde_json::Value = serde_json::from_str(&json_str)
            .unwrap_or_else(|e| panic!("Failed to parse JSON '{json_str}': {e}"));
    }

    // Check expected event sequence: step_started, step_completed for each step, then pipeline_completed.
    let types: Vec<String> = events
        .iter()
        .filter_map(|e| {
            let json: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(e).unwrap()).unwrap();
            json.get("type").and_then(|t| t.as_str().map(String::from))
        })
        .collect();

    assert!(
        types.contains(&"step_started".to_string()),
        "Expected step_started event, got: {types:?}"
    );
    assert!(
        types.contains(&"step_completed".to_string()),
        "Expected step_completed event, got: {types:?}"
    );
    assert!(
        types.last() == Some(&"pipeline_completed".to_string()),
        "Expected pipeline_completed as last event, got: {types:?}"
    );

    std::env::set_current_dir(orig).unwrap();
}

/// SPEC §23 — step_started events include step_id and step_index.
#[test]
fn step_started_event_has_correct_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();

    let pipeline = Pipeline {
        steps: vec![prompt_step("review", "Review the code")],
        defaults: Default::default(),
        source: None,
    };
    let mut session = Session::new(pipeline, "user prompt".to_string());
    let runner = StubRunner::new("ok");

    let (event_tx, event_rx) = mpsc::channel();
    let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let control = ExecutionControl {
        pause_requested: Arc::new(AtomicBool::new(false)),
        kill_requested: Arc::new(AtomicBool::new(false)),
        permission_responder: None,
    };

    execute_with_control(
        &mut session,
        &runner,
        &control,
        &HashSet::new(),
        event_tx,
        hitl_rx,
    )
    .unwrap();

    let events: Vec<ExecutorEvent> = event_rx.iter().collect();
    let first_json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&events[0]).unwrap()).unwrap();

    assert_eq!(first_json["type"], "step_started");
    assert_eq!(first_json["step_id"], "review");
    assert_eq!(first_json["step_index"], 0);
    assert_eq!(first_json["total_steps"], 1);

    std::env::set_current_dir(orig).unwrap();
}

// ── Golden fixture contract tests ────────────────────────────────────────────
//
// For every AilEvent variant we:
//   1. Construct a canonical Rust value
//   2. Serialize it to a serde_json::Value
//   3. Read the corresponding golden fixture from spec/fixtures/events/
//   4. Assert the two Values are equal
//
// If a Rust field is renamed or its type changes, the serialized Value will
// differ from the committed fixture and the test fails, catching wire-format
// drift before it can silently break the VS Code extension.

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..") // workspace root
        .join("spec")
        .join("fixtures")
        .join("events")
}

fn load_fixture(name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {name}: {e}"));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse fixture {name}: {e}"))
}

fn serialize_event(event: &ExecutorEvent) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(event).unwrap()).unwrap()
}

fn serialize_runner_wrapped(inner: RunnerEvent) -> serde_json::Value {
    serialize_event(&ExecutorEvent::RunnerEvent { event: inner })
}

/// SPEC §23 — step_started serialization matches golden fixture.
#[test]
fn golden_step_started() {
    let event = ExecutorEvent::StepStarted {
        step_id: "review".into(),
        step_index: 0,
        total_steps: 3,
        resolved_prompt: Some("Please review the following code for correctness and style.".into()),
    };
    assert_eq!(serialize_event(&event), load_fixture("step_started.json"));
}

/// SPEC §23 — step_started with null resolved_prompt matches golden fixture.
#[test]
fn golden_step_started_no_prompt() {
    let event = ExecutorEvent::StepStarted {
        step_id: "context_step".into(),
        step_index: 1,
        total_steps: 3,
        resolved_prompt: None,
    };
    assert_eq!(
        serialize_event(&event),
        load_fixture("step_started_no_prompt.json")
    );
}

/// SPEC §23 — step_completed with cost matches golden fixture.
#[test]
fn golden_step_completed() {
    let event = ExecutorEvent::StepCompleted {
        step_id: "review".into(),
        cost_usd: Some(0.0042),
        input_tokens: 100,
        output_tokens: 50,
        response: Some("LGTM \u{2014} no issues found.".into()),
        model: None,
    };
    assert_eq!(serialize_event(&event), load_fixture("step_completed.json"));
}

/// SPEC §23 — step_completed with null cost/response matches golden fixture.
#[test]
fn golden_step_completed_no_cost() {
    let event = ExecutorEvent::StepCompleted {
        step_id: "context_step".into(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        response: None,
        model: None,
    };
    assert_eq!(
        serialize_event(&event),
        load_fixture("step_completed_no_cost.json")
    );
}

/// SPEC §23 — step_skipped matches golden fixture.
#[test]
fn golden_step_skipped() {
    let event = ExecutorEvent::StepSkipped {
        step_id: "optional_step".into(),
    };
    assert_eq!(serialize_event(&event), load_fixture("step_skipped.json"));
}

/// SPEC §23 — step_failed matches golden fixture.
#[test]
fn golden_step_failed() {
    let event = ExecutorEvent::StepFailed {
        step_id: "review".into(),
        error: "ail:runner/invocation-failed \u{2014} Runner exited with code 1".into(),
    };
    assert_eq!(serialize_event(&event), load_fixture("step_failed.json"));
}

/// SPEC §23 — hitl_gate_reached matches golden fixture.
#[test]
fn golden_hitl_gate_reached() {
    let event = ExecutorEvent::HitlGateReached {
        step_id: "approval_gate".into(),
        message: None,
    };
    assert_eq!(
        serialize_event(&event),
        load_fixture("hitl_gate_reached.json")
    );
}

/// SPEC §23 — pipeline_completed (completed outcome) matches golden fixture.
#[test]
fn golden_pipeline_completed() {
    let event = ExecutorEvent::PipelineCompleted(ExecuteOutcome::Completed);
    assert_eq!(
        serialize_event(&event),
        load_fixture("pipeline_completed.json")
    );
}

/// SPEC §23 — pipeline_completed (break outcome) matches golden fixture.
#[test]
fn golden_pipeline_completed_break() {
    let event = ExecutorEvent::PipelineCompleted(ExecuteOutcome::Break {
        step_id: "early_exit".into(),
    });
    assert_eq!(
        serialize_event(&event),
        load_fixture("pipeline_completed_break.json")
    );
}

/// SPEC §23 — pipeline_error matches golden fixture.
#[test]
fn golden_pipeline_error() {
    let event = ExecutorEvent::PipelineError {
        error: "Template variable 'undefined_var' is not defined".into(),
        error_type: "ail:template/unresolved-variable".into(),
    };
    assert_eq!(serialize_event(&event), load_fixture("pipeline_error.json"));
}

/// SPEC §23 — runner_event(stream_delta) matches golden fixture.
#[test]
fn golden_runner_event_stream_delta() {
    let inner = RunnerEvent::StreamDelta {
        text: "Hello, world!".into(),
    };
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_stream_delta.json")
    );
}

/// SPEC §23 — runner_event(thinking) matches golden fixture.
#[test]
fn golden_runner_event_thinking() {
    let inner = RunnerEvent::Thinking {
        text: "Let me analyze the code structure first...".into(),
    };
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_thinking.json")
    );
}

/// SPEC §23 — runner_event(tool_use) matches golden fixture.
#[test]
fn golden_runner_event_tool_use() {
    let inner = RunnerEvent::ToolUse {
        tool_name: "Bash".into(),
    };
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_tool_use.json")
    );
}

/// SPEC §23 — runner_event(tool_result) matches golden fixture.
#[test]
fn golden_runner_event_tool_result() {
    let inner = RunnerEvent::ToolResult {
        tool_name: "Bash".into(),
        tool_use_id: None,
        content: None,
        is_error: None,
    };
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_tool_result.json")
    );
}

/// SPEC §23 — runner_event(cost_update) matches golden fixture.
#[test]
fn golden_runner_event_cost_update() {
    let inner = RunnerEvent::CostUpdate {
        cost_usd: 0.0021,
        input_tokens: 50,
        output_tokens: 25,
    };
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_cost_update.json")
    );
}

/// SPEC §23 — runner_event(permission_requested) matches golden fixture.
#[test]
fn golden_runner_event_permission_requested() {
    let inner = RunnerEvent::PermissionRequested(PermissionRequest {
        display_name: "Bash".into(),
        display_detail: "rm -rf /tmp/test-build".into(),
    });
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_permission_requested.json")
    );
}

/// SPEC §23 — runner_event(completed) matches golden fixture.
#[test]
fn golden_runner_event_completed() {
    let inner = RunnerEvent::Completed(RunResult {
        response: "All done!".into(),
        cost_usd: Some(0.0042),
        session_id: Some("ses_abc123".into()),
        input_tokens: 100,
        output_tokens: 50,
        thinking: None,
        model: None,
        tool_events: vec![],
    });
    assert_eq!(
        serialize_runner_wrapped(inner),
        load_fixture("runner_event_completed.json")
    );
}
