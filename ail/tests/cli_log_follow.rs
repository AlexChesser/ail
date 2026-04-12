//! End-to-end tests for `ail log [RUN_ID]` including `--follow` mode.
//!
//! Each test uses an isolated HOME and creates a run via `ail --once` first.

mod common;

use predicates::prelude::*;

/// Helper: run `ail --once "hello" --output-format json` and extract the run_id.
fn create_run(home: &std::path::Path) -> String {
    let mut cmd = common::ail_cmd(home);
    cmd.args(["--once", "hello", "--output-format", "json"]);

    let output = cmd.output().expect("failed to run ail --once");
    assert!(
        output.status.success(),
        "ail --once should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if event["type"].as_str() == Some("run_started") {
                if let Some(run_id) = event["run_id"].as_str() {
                    return run_id.to_string();
                }
            }
        }
    }
    panic!("Could not find run_id in output:\n{stdout}");
}

// ── Non-follow mode ──────────────────────────────────────────────────────────

#[test]
fn log_non_follow_prints_markdown_and_exits() {
    let home = common::isolated_home();
    let run_id = create_run(home.path());

    let mut cmd = common::ail_cmd(home.path());
    cmd.args(["log", &run_id]);

    // Default format is markdown — should contain some content and exit
    cmd.assert()
        .success()
        .stdout(predicates::str::is_empty().not());
}

#[test]
fn log_json_format_produces_valid_ndjson() {
    let home = common::isolated_home();
    let run_id = create_run(home.path());

    let mut cmd = common::ail_cmd(home.path());
    cmd.args(["log", &run_id, "--format", "json"]);

    let output = cmd.output().expect("failed to run ail log");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().filter(|l| !l.is_empty()) {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "Expected valid JSON line, got: {line}");
    }
}

#[test]
fn log_invalid_run_id_exits_nonzero() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["log", "nonexistent-run-id-zzz"]);

    cmd.assert().failure();
}

// ── --follow mode (completed run exits immediately) ──────────────────────────

#[test]
fn log_follow_completed_run_prints_and_exits() {
    let home = common::isolated_home();
    let run_id = create_run(home.path());

    let mut cmd = common::ail_cmd(home.path());
    cmd.args(["log", &run_id, "--follow"]);

    // Since the run is already complete, --follow should print the state and exit
    cmd.assert()
        .success()
        .stdout(predicates::str::is_empty().not());
}
