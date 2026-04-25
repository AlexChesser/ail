//! End-to-end tests for `ail stdio`.
//!
//! Uses the stub runner (`AIL_DEFAULT_RUNNER=stub`) and isolated HOME directories.
//! Interactive stdin-driven tests use `write_stdin()` to pipe input.

mod common;

#[test]
fn stdio_message_flag_text_mode_one_shot() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["stdio", "-m", "hello"]);

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("stub response"));
}

#[test]
fn stdio_message_flag_stream_mode_emits_ndjson() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["stdio", "-m", "hello", "--stream"]);

    let output = cmd.output().expect("failed to run ail stdio --stream");
    assert!(
        output.status.success(),
        "Expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Every non-empty line should be valid JSON
    for line in stdout.lines().filter(|l| !l.is_empty()) {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "Expected valid JSON line, got: {line}");
    }

    // chat_started / chat_ended event names are intentionally unchanged — per
    // issue #183, internal protocol field names are a follow-up.
    assert!(
        stdout.contains("chat_started"),
        "Expected chat_started event in output"
    );
    assert!(
        stdout.contains("chat_ended"),
        "Expected chat_ended event in output"
    );
}

#[test]
fn stdio_eof_on_empty_stdin_exits_cleanly() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["stdio"]);
    // Close stdin immediately by writing empty bytes
    cmd.write_stdin(b"" as &[u8]);

    cmd.assert().success();
}

#[test]
fn stdio_single_prompt_via_stdin_gets_response() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["stdio"]);
    // Send a prompt and immediately close stdin (EOF)
    cmd.write_stdin("hello\n");

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("stub response"));
}
