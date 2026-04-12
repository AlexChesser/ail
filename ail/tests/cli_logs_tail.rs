//! End-to-end tests for `ail logs` including `--tail` mode.
//!
//! Each test uses an isolated HOME. Non-tail tests create runs first and verify output.
//! The `--tail` test spawns the process and kills it after a bounded timeout.

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

// ── Non-tail mode ────────────────────────────────────────────────────────────

#[test]
fn logs_lists_sessions_and_exits() {
    let home = common::isolated_home();
    let run_id = create_run(home.path());

    let mut cmd = common::ail_cmd(home.path());
    cmd.args(["logs"]);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains(&run_id));
}

#[test]
fn logs_limit_caps_output() {
    let home = common::isolated_home();
    let _run1 = create_run(home.path());
    let run2 = create_run(home.path());

    let mut cmd = common::ail_cmd(home.path());
    cmd.args(["logs", "--limit", "1"]);

    let output = cmd.output().expect("failed to run ail logs");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // With limit 1, should contain at most 1 run_id reference
    let run_id_count = stdout.lines().filter(|l| l.contains("run_id:")).count();
    assert!(
        run_id_count <= 1,
        "Expected at most 1 session with --limit 1, got {run_id_count} run_id lines.\n\
         Most recent run: {run2}"
    );
}

#[test]
fn logs_json_format_produces_valid_ndjson() {
    let home = common::isolated_home();
    let _run_id = create_run(home.path());

    let mut cmd = common::ail_cmd(home.path());
    cmd.args(["logs", "--format", "json"]);

    let output = cmd.output().expect("failed to run ail logs");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().filter(|l| !l.is_empty()) {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "Expected valid JSON line, got: {line}");
    }
}

// ── --tail mode (infinite loop — spawn and kill) ─────────────────────────────

#[test]
fn logs_tail_emits_existing_then_can_be_killed() {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let home = common::isolated_home();
    let run_id = create_run(home.path());

    let bin = assert_cmd::cargo::cargo_bin("ail");
    let mut child = Command::new(bin)
        .args(["logs", "--tail"])
        .env("AIL_DEFAULT_RUNNER", "stub")
        .env("HOME", home.path())
        .current_dir(home.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ail logs --tail");

    // Wait a moment for the initial output
    std::thread::sleep(Duration::from_millis(500));

    // Kill the process
    let _ = child.kill();
    let output = child.wait_with_output().expect("failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have emitted the existing run
    assert!(
        stdout.contains(&run_id),
        "Expected run_id {run_id} in tail output, got: {stdout}"
    );
}
