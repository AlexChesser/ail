//! Shell subprocess execution for context steps.

#![allow(clippy::result_large_err)]

use std::process::{Command, Stdio};

use crate::error::AilError;

/// Spawn `/bin/sh -c cmd` and return `(stdout, stderr, exit_code)`.
pub(in crate::executor) fn run_shell_command(
    run_id: &str,
    step_id: &str,
    cmd: &str,
) -> Result<(String, String, i32), AilError> {
    let child = Command::new("/bin/sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AilError::RunnerInvocationFailed {
            detail: format!("Could not run shell command for step '{step_id}': {e}"),
            context: Some(crate::error::ErrorContext::for_step(run_id, step_id)),
        })?;

    let output = child
        .wait_with_output()
        .map_err(|e| AilError::RunnerInvocationFailed {
            detail: format!("Step '{step_id}': {e}"),
            context: Some(crate::error::ErrorContext::for_step(run_id, step_id)),
        })?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((stdout, stderr, exit_code))
}
