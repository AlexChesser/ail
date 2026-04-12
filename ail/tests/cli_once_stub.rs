//! End-to-end tests for `ail --once` with the stub runner.

mod common;

use predicates::prelude::*;

#[test]
fn once_stub_text_prints_response() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["--once", "hello"]);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("stub response"));
}

#[test]
fn once_stub_json_event_stream_ordered() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["--once", "hello", "--output-format", "json"]);

    let output = cmd.output().expect("failed to run ail --once");
    assert!(output.status.success(), "Expected success exit code");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("each line should be valid JSON"))
        .collect();

    assert!(
        !events.is_empty(),
        "Expected at least one NDJSON event, got none"
    );

    // Verify each line has a "type" field
    for (i, event) in events.iter().enumerate() {
        assert!(
            event.get("type").is_some(),
            "Event {i} missing 'type' field: {event}"
        );
    }

    // Check event ordering: run_started should be first
    let types: Vec<&str> = events.iter().filter_map(|e| e["type"].as_str()).collect();
    assert_eq!(
        types.first(),
        Some(&"run_started"),
        "First event should be run_started, got: {types:?}"
    );
}

#[test]
fn once_stub_with_multi_step_pipeline_runs_all() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["--once", "hello", "--pipeline"])
        .arg(common::fixture_path("solo_developer.ail.yaml"))
        .args(["--output-format", "json"]);

    let output = cmd.output().expect("failed to run ail --once");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let step_completed_count = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|e| e["type"].as_str() == Some("step_completed"))
        .count();

    // solo_developer.ail.yaml has 2 pipeline steps + the implicit invocation step = 3 total
    assert_eq!(
        step_completed_count, 3,
        "Expected 3 step_completed events (invocation + 2 pipeline steps)"
    );
}

#[test]
fn once_stub_unknown_runner_exits_1() {
    let (mut cmd, home) = common::ail_cmd_isolated();
    cmd.env("AIL_DEFAULT_RUNNER", "nope");
    cmd.current_dir(home.path());
    cmd.args(["--once", "hello"]);

    cmd.assert()
        .failure()
        .stderr(predicates::str::is_empty().not());
}

#[test]
fn once_stub_model_flag_passes_through() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["--once", "hello", "--model", "gemma3:1b"]);

    // Stub runner is model-insensitive; just verify no parse error
    cmd.assert().success();
}
