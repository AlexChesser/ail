use ail_core::config::domain::{Pipeline, Step, StepBody, StepId};
use ail_core::executor::{execute_with_control, ExecutionControl, ExecutorEvent};
use ail_core::runner::stub::StubRunner;
use ail_core::session::Session;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

fn prompt_step(id: &str, text: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
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
