//! Context shell step dispatch — shell subprocess execution and TurnEntry construction.

#![allow(clippy::result_large_err)]

use crate::error::AilError;
use crate::session::{Session, TurnEntry};

use crate::executor::core::StepObserver;
use crate::executor::helpers::run_shell_command;

/// Execute a shell context step: run the command and return a TurnEntry.
pub(in crate::executor) fn execute_shell<O: StepObserver>(
    cmd: &str,
    session: &mut Session,
    step_id: &str,
    observer: &mut O,
) -> Result<TurnEntry, AilError> {
    session.turn_log.record_step_started(step_id, cmd);
    let (stdout, stderr, exit_code) = run_shell_command(&session.run_id, step_id, cmd)
        .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;
    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        exit_code,
        "context shell step complete"
    );
    observer.on_non_prompt_completed(step_id);
    Ok(TurnEntry::from_context(
        step_id.to_string(),
        cmd.to_string(),
        stdout,
        stderr,
        exit_code,
    ))
}
