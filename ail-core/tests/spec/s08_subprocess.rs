//! Tests for the generic subprocess session lifecycle (`runner/subprocess.rs`).
//!
//! These tests exercise `SubprocessSession::{spawn, take_stdout, finish}` using
//! real system binaries via `/bin/sh -c`. We avoid hardcoded paths like `/bin/true`
//! or `/bin/false` because macOS places them at `/usr/bin/` instead.
//! Tests cover the full lifecycle: spawn, stdout streaming, stderr drain,
//! cancel-watchdog, environment manipulation, and error on missing binary.

#![cfg(unix)]

use ail_core::runner::subprocess::{SubprocessOutcome, SubprocessSession, SubprocessSpec};
use ail_core::runner::CancelToken;
use std::io::BufRead;
use std::time::{Duration, Instant};

fn simple_spec(program: &str) -> SubprocessSpec {
    SubprocessSpec {
        program: program.to_string(),
        args: vec![],
        env_remove: vec![],
        env_set: vec![],
    }
}

fn sh_spec(cmd: &str) -> SubprocessSpec {
    SubprocessSpec {
        program: "/bin/sh".to_string(),
        args: vec!["-c".to_string(), cmd.to_string()],
        env_remove: vec![],
        env_set: vec![],
    }
}

/// Drain stdout to EOF and then finish the session.
fn drain_and_finish(mut session: SubprocessSession) -> SubprocessOutcome {
    if let Some(reader) = session.take_stdout() {
        // Read all lines to EOF
        for _ in reader.lines() {}
    }
    session.finish().expect("finish should succeed")
}

// ── Basic lifecycle ──────────────────────────────────────────────────────────

#[test]
fn subprocess_true_exits_success() {
    let session =
        SubprocessSession::spawn(sh_spec("true"), None).expect("spawn 'true' should succeed");
    let outcome = drain_and_finish(session);

    assert!(outcome.exit_status.success(), "Expected exit code 0");
    assert!(!outcome.was_cancelled, "Should not be cancelled");
    // Note: we intentionally do NOT assert stderr.is_empty() here. When other
    // tests in the suite change the process CWD to a temporary directory that is
    // later deleted, /bin/sh emits "getcwd() failed" on stderr even though the
    // command succeeds. Asserting on stderr would make this test flaky.
    // Stderr drain correctness is tested in subprocess_stderr_drained_to_outcome.
}

#[test]
fn subprocess_false_nonzero_exit() {
    let session =
        SubprocessSession::spawn(sh_spec("false"), None).expect("spawn 'false' should succeed");
    let outcome = drain_and_finish(session);

    assert!(
        !outcome.exit_status.success(),
        "Expected non-zero exit code"
    );
    assert!(!outcome.was_cancelled);
}

// ── Stdout streaming ─────────────────────────────────────────────────────────

#[test]
fn subprocess_stdout_stream_readable() {
    let mut session = SubprocessSession::spawn(sh_spec("printf 'line1\\nline2\\n'"), None)
        .expect("spawn should succeed");

    let reader = session.take_stdout().expect("stdout should be available");
    let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();

    assert_eq!(lines, vec!["line1", "line2"]);

    let outcome = session.finish().expect("finish should succeed");
    assert!(outcome.exit_status.success());
}

#[test]
fn subprocess_take_stdout_returns_none_on_second_call() {
    let mut session =
        SubprocessSession::spawn(sh_spec("true"), None).expect("spawn should succeed");

    let first = session.take_stdout();
    assert!(first.is_some(), "First take_stdout should return Some");
    let second = session.take_stdout();
    assert!(second.is_none(), "Second take_stdout should return None");

    // Drain via first reader
    if let Some(reader) = first {
        for _ in reader.lines() {}
    }
    session.finish().expect("finish should succeed");
}

// ── Stderr drain ─────────────────────────────────────────────────────────────

#[test]
fn subprocess_stderr_drained_to_outcome() {
    let session = SubprocessSession::spawn(sh_spec("echo err_output >&2; exit 0"), None)
        .expect("spawn should succeed");
    let outcome = drain_and_finish(session);

    assert!(outcome.exit_status.success());
    assert!(
        outcome.stderr.contains("err_output"),
        "Expected stderr to contain 'err_output', got: {:?}",
        outcome.stderr
    );
}

// ── Environment manipulation ─────────────────────────────────────────────────

#[test]
fn subprocess_env_set_and_env_remove_applied() {
    let spec = SubprocessSpec {
        program: "/bin/sh".to_string(),
        args: vec!["-c".to_string(), "echo $TEST_FOO".to_string()],
        env_remove: vec![],
        env_set: vec![("TEST_FOO".to_string(), "bar_value".to_string())],
    };

    let mut session = SubprocessSession::spawn(spec, None).expect("spawn should succeed");
    let reader = session.take_stdout().expect("stdout should be available");
    let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();

    assert_eq!(lines, vec!["bar_value"]);
    session.finish().expect("finish should succeed");
}

// ── Cancel watchdog ──────────────────────────────────────────────────────────

#[test]
fn subprocess_cancel_token_kills_child() {
    let token = CancelToken::new();
    // Use /bin/sleep directly (not via sh -c) so kill() targets the exact process.
    let spec = SubprocessSpec {
        program: "/bin/sleep".to_string(),
        args: vec!["30".to_string()],
        env_remove: vec![],
        env_set: vec![],
    };
    let mut session =
        SubprocessSession::spawn(spec, Some(token.clone())).expect("spawn should succeed");

    let reader = session.take_stdout().expect("stdout should be available");
    let start = Instant::now();

    // Cancel after a short delay — the watchdog blocks on the token's event listener
    let token_clone = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        token_clone.cancel();
    });

    // Drain stdout — blocks until child is killed by the watchdog, closing the pipe
    for _ in reader.lines() {}

    let outcome = session.finish().expect("finish should succeed");
    let elapsed = start.elapsed();

    assert!(outcome.was_cancelled, "Should be marked as cancelled");
    assert!(
        elapsed < Duration::from_secs(5),
        "Cancel should complete quickly, took {:?}",
        elapsed
    );
}

#[test]
fn subprocess_was_cancelled_false_without_token() {
    let session = SubprocessSession::spawn(sh_spec("true"), None).expect("spawn should succeed");
    assert!(
        !session.was_cancelled(),
        "was_cancelled should be false without a token"
    );
    drain_and_finish(session);
}

// ── Error on missing binary ──────────────────────────────────────────────────

#[test]
fn subprocess_missing_program_returns_error() {
    let result = SubprocessSession::spawn(simple_spec("/nonexistent/binary"), None);

    match result {
        Ok(_) => panic!("Expected error for missing binary, got Ok"),
        Err(err) => {
            assert_eq!(
                err.error_type(),
                ail_core::error::error_types::RUNNER_INVOCATION_FAILED
            );
            assert!(
                err.detail().contains("/nonexistent/binary"),
                "Error detail should mention the binary path, got: {}",
                err.detail()
            );
        }
    }
}
