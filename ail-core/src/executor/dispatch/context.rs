//! Context step dispatch — shell subprocess and spec query execution.

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

/// Execute a spec context step: resolve the query and return embedded spec content.
pub(in crate::executor) fn execute_spec<O: StepObserver>(
    query: &str,
    session: &mut Session,
    step_id: &str,
    observer: &mut O,
) -> Result<TurnEntry, AilError> {
    session
        .turn_log
        .record_step_started(step_id, &format!("spec:{query}"));
    let content = resolve_spec_query(query).map_err(|detail| {
        observer.on_step_failed(step_id, &detail);
        AilError::config_validation(&detail)
    })?;
    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        query = %query,
        "context spec step complete"
    );
    observer.on_non_prompt_completed(step_id);
    Ok(TurnEntry::from_context(
        step_id.to_string(),
        format!("spec:{query}"),
        content,
        String::new(),
        0,
    ))
}

/// Resolve a spec query string to content.
/// Accepts tier names (`compact`, `schema`, `prose`) or section IDs (`s05`, `r02`).
pub(crate) fn resolve_spec_query(query: &str) -> Result<String, String> {
    match query {
        "compact" => Ok(ail_spec::compact().to_string()),
        "schema" => Ok(ail_spec::schema().to_string()),
        "prose" => Ok(ail_spec::full_prose()),
        "core" => Ok(ail_spec::core_prose()),
        "runner" => Ok(ail_spec::runner_prose()),
        other => ail_spec::section(other)
            .map(|s| s.to_string())
            .ok_or_else(|| {
                format!(
                    "Unknown spec query '{other}'. Use a tier (compact, schema, prose) \
                     or a section ID (s05, r02). Run `ail spec --list` to see available sections."
                )
            }),
    }
}
