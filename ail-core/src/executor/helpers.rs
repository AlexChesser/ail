//! Shared helper functions used by both the headless and controlled execution paths.

#![allow(clippy::result_large_err)]

use std::process::{Command, Stdio};

use crate::config::domain::{
    ExitCodeMatch, ProviderConfig, ResultAction, ResultMatcher, Step, SystemPromptEntry,
};
use crate::error::{error_types, AilError};
use crate::runner::factory::RunnerFactory;
use crate::runner::{InvokeOptions, RunResult, Runner, ToolPermissionPolicy};
use crate::session::{Session, TurnEntry};
use crate::template;

/// Run the host-managed invocation step when the pipeline does not declare its own.
///
/// Calls `runner.invoke()`, appends a `TurnEntry` for `"invocation"`, and returns
/// the `RunResult` so callers can use it for output or event emission.
///
/// **Callers must first check `session.has_invocation_step()`** — this function
/// unconditionally runs the runner and logs the result. It does not recheck.
pub fn run_invocation_step(
    session: &mut Session,
    runner: &dyn Runner,
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

/// Resolve the effective provider config for a step by merging pipeline defaults,
/// step-level model override, and CLI provider flags.
pub(super) fn resolve_step_provider(session: &Session, step: &Step) -> ProviderConfig {
    session
        .pipeline
        .defaults
        .clone()
        .merge(ProviderConfig {
            model: step.model.clone(),
            base_url: None,
            auth_token: None,
            input_cost_per_1k: None,
            output_cost_per_1k: None,
        })
        .merge(session.cli_provider.clone())
}

/// Build a per-step runner override box if `step.runner` is set (SPEC §19).
pub(super) fn build_step_runner_box(
    step: &Step,
) -> Result<Option<Box<dyn Runner + Send>>, AilError> {
    match step.runner {
        Some(ref name) => Ok(Some(RunnerFactory::build(name, true)?)),
        None => Ok(None),
    }
}

/// Spawn `/bin/sh -c cmd` and return `(stdout, stderr, exit_code)`.
pub(super) fn run_shell_command(
    run_id: &str,
    step_id: &str,
    cmd: &str,
) -> Result<(String, String, i32), AilError> {
    let child = Command::new("/bin/sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AilError {
            error_type: error_types::RUNNER_INVOCATION_FAILED,
            title: "Failed to spawn shell command",
            detail: format!("Could not run shell command for step '{step_id}': {e}"),
            context: Some(crate::error::ErrorContext::for_step(run_id, step_id)),
        })?;

    let output = child.wait_with_output().map_err(|e| AilError {
        error_type: error_types::RUNNER_INVOCATION_FAILED,
        title: "Failed to wait for shell command",
        detail: format!("Step '{step_id}': {e}"),
        context: Some(crate::error::ErrorContext::for_step(run_id, step_id)),
    })?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((stdout, stderr, exit_code))
}

/// Evaluate `on_result` branches against the most recent `TurnEntry`.
/// Returns the action of the first matching branch, or `None` if no branch matches.
pub(super) fn evaluate_on_result(
    branches: &[crate::config::domain::ResultBranch],
    entry: &TurnEntry,
) -> Option<ResultAction> {
    for branch in branches {
        let matched = match &branch.matcher {
            ResultMatcher::Contains(text) => {
                let haystack = entry
                    .response
                    .as_deref()
                    .or(entry.stdout.as_deref())
                    .unwrap_or("");
                let haystack_lower = haystack.to_lowercase();
                haystack_lower.contains(&text.to_lowercase())
            }
            ResultMatcher::ExitCode(ExitCodeMatch::Exact(n)) => entry.exit_code == Some(*n),
            ResultMatcher::ExitCode(ExitCodeMatch::Any) => {
                // `any` matches any non-zero exit code — does NOT match 0.
                matches!(entry.exit_code, Some(c) if c != 0)
            }
            ResultMatcher::Always => true,
        };

        if matched {
            return Some(branch.action.clone());
        }
    }
    None
}

/// Build a `ToolPermissionPolicy` from an optional `ToolPolicy` domain value.
pub(super) fn build_tool_policy(
    tools: Option<&crate::config::domain::ToolPolicy>,
) -> ToolPermissionPolicy {
    match tools {
        Some(t) if t.disabled => ToolPermissionPolicy::NoTools,
        Some(t) if !t.allow.is_empty() && !t.deny.is_empty() => ToolPermissionPolicy::Mixed {
            allow: t.allow.clone(),
            deny: t.deny.clone(),
        },
        Some(t) if !t.allow.is_empty() => ToolPermissionPolicy::Allowlist(t.allow.clone()),
        Some(t) if !t.deny.is_empty() => ToolPermissionPolicy::Denylist(t.deny.clone()),
        _ => ToolPermissionPolicy::RunnerDefault,
    }
}

/// Resolve `system_prompt` and all `append_system_prompt` entries for a step.
///
/// Returns `(resolved_system_prompt, resolved_append_entries)` on success.
/// On error, returns an `AilError` with step context already populated — the caller
/// may wrap with event emission before propagating.
pub(super) fn resolve_step_system_prompts(
    step: &Step,
    session: &Session,
    step_id: &str,
    pipeline_base_dir: Option<&std::path::Path>,
) -> Result<(Option<String>, Vec<String>), AilError> {
    let resolved_system_prompt = step
        .system_prompt
        .as_deref()
        .map(|sp| {
            let content = resolve_prompt_file(sp, step_id, pipeline_base_dir)?;
            template::resolve(&content, session)
                .map_err(|e| e.with_step_context(&session.run_id, step_id))
        })
        .transpose()?;

    let mut resolved_append: Vec<String> = Vec::new();
    if let Some(entries) = &step.append_system_prompt {
        for entry in entries {
            let text = match entry {
                SystemPromptEntry::Text(s) => template::resolve(s, session)
                    .map_err(|e| e.with_step_context(&session.run_id, step_id))?,
                SystemPromptEntry::File(path) => {
                    let content = std::fs::read_to_string(path).map_err(|e| AilError {
                        error_type: error_types::CONFIG_FILE_NOT_FOUND,
                        title: "append_system_prompt file not found",
                        detail: format!(
                            "Step '{step_id}' append_system_prompt file '{}' could not be read: {e}",
                            path.display()
                        ),
                        context: Some(crate::error::ErrorContext::for_step(&session.run_id, step_id)),
                    })?;
                    template::resolve(&content, session)
                        .map_err(|e| e.with_step_context(&session.run_id, step_id))?
                }
                SystemPromptEntry::Shell(cmd) => {
                    let resolved_cmd = template::resolve(cmd, session)
                        .map_err(|e| e.with_step_context(&session.run_id, step_id))?;
                    let (stdout, _stderr, _exit_code) =
                        run_shell_command(&session.run_id, step_id, &resolved_cmd)?;
                    stdout
                }
            };
            resolved_append.push(text);
        }
    }
    Ok((resolved_system_prompt, resolved_append))
}

/// If `prompt_text` starts with a path prefix (`./`, `../`, `~/`, `/`), read the file
/// at that path and return its contents as the prompt template. Otherwise returns the
/// original string unchanged. `~/` is expanded using the user's home directory.
///
/// `base_dir` is the directory of the pipeline file that owns this step. Per SPEC §5.2,
/// `./` and `../` paths are resolved relative to the pipeline file, not the process CWD.
pub(super) fn resolve_prompt_file(
    prompt_text: &str,
    step_id: &str,
    base_dir: Option<&std::path::Path>,
) -> Result<String, AilError> {
    let is_path = prompt_text.starts_with("./")
        || prompt_text.starts_with("../")
        || prompt_text.starts_with("~/")
        || prompt_text.starts_with('/');

    if !is_path {
        return Ok(prompt_text.to_string());
    }

    let path = if let Some(rel) = prompt_text.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| AilError {
            error_type: error_types::CONFIG_FILE_NOT_FOUND,
            title: "Cannot resolve home directory",
            detail: format!(
                "Step '{step_id}' prompt path starts with ~/ but home dir is unavailable"
            ),
            context: None,
        })?;
        home.join(rel)
    } else if prompt_text.starts_with('/') {
        std::path::PathBuf::from(prompt_text)
    } else if let Some(base) = base_dir {
        // ./  or ../  — resolve relative to the pipeline file's directory (SPEC §5.2)
        base.join(prompt_text)
    } else {
        // No base_dir available (e.g. passthrough pipeline) — fall back to CWD-relative.
        std::path::PathBuf::from(prompt_text)
    };

    std::fs::read_to_string(&path).map_err(|e| AilError {
        error_type: error_types::CONFIG_FILE_NOT_FOUND,
        title: "Prompt file not found",
        detail: format!(
            "Step '{step_id}' prompt file '{}' could not be read: {e}",
            path.display()
        ),
        context: None,
    })
}
