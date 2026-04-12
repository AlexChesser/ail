//! Generic subprocess session for CLI-based runner implementations.
//!
//! [`SubprocessSession`] owns the full lifecycle of a streaming child process: spawning,
//! stdout access, concurrent stderr drain, optional cancel-watchdog, and clean reaping.
//!
//! HTTP and in-process runners do not use this module — it is purely the reusable substrate
//! for the family of runners that shell out to a CLI (Claude, Codex, Aider, Gemini, etc.).

#![allow(clippy::result_large_err)]

use std::io::{BufReader, Read};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use event_listener::Listener;

use super::CancelToken;
use crate::error::AilError;

/// Specification for spawning a subprocess.
pub struct SubprocessSpec {
    /// Executable name or absolute path.
    pub program: String,
    /// Command-line arguments in order.
    pub args: Vec<String>,
    /// Environment variable names to remove from the child process environment.
    pub env_remove: Vec<String>,
    /// Environment variable name-value pairs to set in the child process environment.
    pub env_set: Vec<(String, String)>,
}

/// Outcome returned by [`SubprocessSession::finish`].
pub struct SubprocessOutcome {
    pub exit_status: ExitStatus,
    /// All bytes written to stderr, collected after stdout is drained.
    pub stderr: String,
    /// True if the cancel token was set at any point during the session.
    pub was_cancelled: bool,
}

/// Owns the lifecycle of a streaming subprocess.
///
/// After [`SubprocessSession::spawn`] returns, call [`SubprocessSession::take_stdout`] to
/// obtain the stdout reader, iterate lines, then call [`SubprocessSession::finish`] to stop
/// the watchdog, reap the child, and collect stderr.
///
/// # Cancellation
///
/// If `cancel_token` is `Some`, a watchdog thread blocks on the token's event listener
/// (no polling). When `cancel()` is called, the watchdog wakes and kills the child.
/// On normal completion, [`SubprocessSession::finish`] calls `wake()` on the token to
/// cleanly shut down the watchdog. The caller should check
/// [`SubprocessOutcome::was_cancelled`] before interpreting a non-zero exit status, since a
/// killed child exits non-zero.
pub struct SubprocessSession {
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    stderr_handle: Option<JoinHandle<String>>,
    watchdog: Option<JoinHandle<()>>,
    cancel_token: Option<CancelToken>,
}

impl SubprocessSession {
    /// Spawn the subprocess described by `spec`.
    ///
    /// Stderr is drained on a background thread immediately to prevent pipe-buffer deadlock
    /// (a child that fills its stderr pipe before the parent reads it will block forever).
    ///
    /// If `cancel_token` is `Some`, a watchdog thread is started that kills the child when
    /// the token is cancelled.
    pub fn spawn(
        spec: SubprocessSpec,
        cancel_token: Option<CancelToken>,
    ) -> Result<Self, AilError> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        for name in &spec.env_remove {
            cmd.env_remove(name);
        }
        for (k, v) in &spec.env_set {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AilError::RunnerInvocationFailed {
                detail: format!("Could not start '{}': {e}", spec.program),
                context: None,
            })?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        // Drain stderr concurrently to prevent pipe-buffer deadlock.
        let stderr_handle = thread::spawn(move || {
            let mut s = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut s);
            s
        });

        let child = Arc::new(Mutex::new(child));

        // Watchdog: blocks on the cancel token's event listener (no polling).
        // On cancel: kills the child. On normal completion: finish() calls wake()
        // to unblock the listener for clean shutdown.
        let watchdog = cancel_token.as_ref().map(|token| {
            let token = token.clone();
            let child_w = Arc::clone(&child);
            thread::spawn(move || {
                let listener = token.listen();
                // Double-check after registering to avoid race with already-cancelled token.
                if token.is_cancelled() {
                    tracing::info!("cancel_token set — killing runner subprocess");
                    if let Ok(mut c) = child_w.lock() {
                        let _ = c.kill();
                    }
                    return;
                }
                listener.wait();
                if token.is_cancelled() {
                    tracing::info!("cancel_token set — killing runner subprocess");
                    if let Ok(mut c) = child_w.lock() {
                        let _ = c.kill();
                    }
                }
                // else: wake() was called for normal completion — exit cleanly
            })
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: Some(BufReader::new(stdout)),
            stderr_handle: Some(stderr_handle),
            watchdog,
            cancel_token,
        })
    }

    /// Take the child's stdin handle. May only be called once — subsequent calls return `None`.
    ///
    /// Used by runners that need to write to the child process (e.g. ProtocolRunner sends
    /// JSON-RPC requests on stdin). Runners that only read stdout (e.g. ClaudeCliRunner) do
    /// not need this — dropping the handle is fine.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.stdin.take()
    }

    /// Take the buffered stdout reader. May only be called once — subsequent calls return `None`.
    pub fn take_stdout(&mut self) -> Option<BufReader<ChildStdout>> {
        self.stdout.take()
    }

    /// Whether cancellation was requested at any point during this session.
    ///
    /// Must be checked **before** interpreting a non-zero exit status: a killed child exits
    /// non-zero, which would otherwise be misreported as an unexpected failure.
    pub fn was_cancelled(&self) -> bool {
        self.cancel_token
            .as_ref()
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
    }

    /// Stop the watchdog, wait for the child to exit, and join the stderr drain thread.
    ///
    /// Returns the exit status, collected stderr output, and whether cancellation was
    /// requested. After this call the session is consumed.
    pub fn finish(self) -> Result<SubprocessOutcome, AilError> {
        // Wake the watchdog for clean shutdown (wake without cancel).
        if let Some(ref token) = self.cancel_token {
            token.wake();
        }

        let was_cancelled = self
            .cancel_token
            .as_ref()
            .map(|t| t.is_cancelled())
            .unwrap_or(false);

        if let Some(h) = self.watchdog {
            let _ = h.join();
        }

        let exit_status = self
            .child
            .lock()
            .expect("child mutex not poisoned")
            .wait()
            .map_err(|e| AilError::RunnerInvocationFailed {
                detail: e.to_string(),
                context: None,
            })?;

        let stderr = self
            .stderr_handle
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default();

        Ok(SubprocessOutcome {
            exit_status,
            stderr,
            was_cancelled,
        })
    }
}
