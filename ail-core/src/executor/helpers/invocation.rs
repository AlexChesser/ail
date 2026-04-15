//! Invocation-step lifecycle — running the host-managed invocation step.

#![allow(clippy::result_large_err)]

use crate::error::AilError;
use crate::runner::{InvokeOptions, RunResult, Runner};
use crate::session::{Session, TurnEntry};

/// Run the host-managed invocation step when the pipeline does not declare its own.
///
/// Calls `runner.invoke()`, appends a `TurnEntry` for `"invocation"`, and returns
/// the `RunResult` so callers can use it for output or event emission.
///
/// **Callers must first check `session.has_invocation_step()`** — this function
/// unconditionally runs the runner and logs the result. It does not recheck.
pub fn run_invocation_step(
    session: &mut Session,
    runner: &(dyn Runner + Sync),
    prompt: &str,
    options: InvokeOptions,
) -> Result<RunResult, AilError> {
    session.turn_log.record_step_started("invocation", prompt);
    let result = runner
        .invoke(prompt, options)
        .map_err(|e| e.with_step_context(&session.run_id, "invocation"))?;
    let result_clone = result.clone();
    session.turn_log.append(TurnEntry::from_prompt(
        "invocation",
        prompt.to_string(),
        result,
    ));
    Ok(result_clone)
}
