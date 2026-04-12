//! Shared helper functions used by both the headless and controlled execution paths.

#![allow(clippy::result_large_err)]

use std::process::{Command, Stdio};

use crate::config::domain::{
    ExitCodeMatch, ProviderConfig, ResultAction, ResultMatcher, Step, SystemPromptEntry,
};
use crate::error::AilError;
use crate::runner::factory::RunnerFactory;
use crate::runner::http::HttpSessionStore;
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
            ..Default::default()
        })
        .merge(session.cli_provider.clone())
}

/// Build a per-step runner override box if `step.runner` is set (SPEC §19).
///
/// `headless` is propagated from `Session.headless` so per-step `runner: claude` overrides
/// honour the same `--dangerously-skip-permissions` flag as the default runner.
pub(super) fn build_step_runner_box(
    step: &Step,
    headless: bool,
    http_store: &HttpSessionStore,
    provider: &ProviderConfig,
) -> Result<Option<Box<dyn Runner + Send>>, AilError> {
    match step.runner {
        Some(ref name) => Ok(Some(RunnerFactory::build(
            name, headless, http_store, provider,
        )?)),
        None => Ok(None),
    }
}

/// Resolve the effective runner name for a step without constructing the runner.
///
/// Mirrors the `RunnerFactory` selection hierarchy:
/// 1. Per-step `runner:` field
/// 2. `AIL_DEFAULT_RUNNER` environment variable
/// 3. `"claude"` fallback
///
/// Used to update `session.runner_name` so `{{ session.tool }}` reflects the actual runner.
pub(super) fn resolve_effective_runner_name(step: &Step) -> String {
    if let Some(ref name) = step.runner {
        name.clone()
    } else {
        std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string())
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
                    let content = std::fs::read_to_string(path).map_err(|e| AilError::ConfigFileNotFound {
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
        let home = dirs::home_dir().ok_or_else(|| AilError::ConfigFileNotFound {
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

    std::fs::read_to_string(&path).map_err(|e| AilError::ConfigFileNotFound {
        detail: format!(
            "Step '{step_id}' prompt file '{}' could not be read: {e}",
            path.display()
        ),
        context: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{Pipeline, Step, StepBody, StepId, SystemPromptEntry, ToolPolicy};
    use crate::runner::ToolPermissionPolicy;
    use crate::session::log_provider::NullProvider;
    use crate::session::Session;

    // ── build_tool_policy ────────────────────────────────────────────────────

    fn make_policy(disabled: bool, allow: Vec<&str>, deny: Vec<&str>) -> ToolPolicy {
        ToolPolicy {
            disabled,
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn build_tool_policy_none_returns_runner_default() {
        assert!(matches!(
            build_tool_policy(None),
            ToolPermissionPolicy::RunnerDefault
        ));
    }

    #[test]
    fn build_tool_policy_disabled_returns_no_tools() {
        let policy = make_policy(true, vec![], vec![]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::NoTools
        ));
    }

    #[test]
    fn build_tool_policy_allow_and_deny_returns_mixed() {
        let policy = make_policy(false, vec!["Bash"], vec!["Write"]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::Mixed { .. }
        ));
    }

    #[test]
    fn build_tool_policy_allow_only_returns_allowlist() {
        let policy = make_policy(false, vec!["Read", "Bash"], vec![]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::Allowlist(_)
        ));
    }

    #[test]
    fn build_tool_policy_deny_only_returns_denylist() {
        let policy = make_policy(false, vec![], vec!["Bash"]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::Denylist(_)
        ));
    }

    #[test]
    fn build_tool_policy_empty_returns_runner_default() {
        let policy = make_policy(false, vec![], vec![]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::RunnerDefault
        ));
    }

    // ── resolve_prompt_file ──────────────────────────────────────────────────

    #[test]
    fn resolve_prompt_file_plain_text_returns_unchanged() {
        let text = "just a plain string with no path prefix";
        let result = resolve_prompt_file(text, "step", None).unwrap();
        assert_eq!(result, text);
    }

    #[test]
    fn resolve_prompt_file_absolute_path_reads_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "absolute content").unwrap();
        let path_str = tmp.path().to_string_lossy().to_string();
        let result = resolve_prompt_file(&path_str, "step", None).unwrap();
        assert_eq!(result, "absolute content");
    }

    #[test]
    fn resolve_prompt_file_relative_dot_slash_resolves_against_base_dir() {
        let base = tempfile::tempdir().unwrap();
        let file = base.path().join("prompt.txt");
        std::fs::write(&file, "from base dir").unwrap();
        let result = resolve_prompt_file("./prompt.txt", "step", Some(base.path())).unwrap();
        assert_eq!(result, "from base dir");
    }

    #[test]
    fn resolve_prompt_file_relative_dot_dot_resolves_against_base_dir() {
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("sub");
        std::fs::create_dir_all(&child).unwrap();
        let file = parent.path().join("up.txt");
        std::fs::write(&file, "parent content").unwrap();
        let result = resolve_prompt_file("../up.txt", "step", Some(&child)).unwrap();
        assert_eq!(result, "parent content");
    }

    #[test]
    fn resolve_prompt_file_missing_file_returns_config_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let result = resolve_prompt_file("/nonexistent/path/file.txt", "step", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CONFIG_FILE_NOT_FOUND
        );
        let _ = tmp; // ensure tempdir lives long enough
    }

    #[test]
    fn resolve_prompt_file_tilde_prefix_reads_file_in_home() {
        let home = dirs::home_dir().expect("home dir must be available in test environment");
        let tmp = tempfile::Builder::new()
            .prefix("ail_test_")
            .suffix(".txt")
            .tempfile_in(&home)
            .expect("create tempfile in home dir");
        std::fs::write(tmp.path(), "home content").unwrap();
        let relative = tmp
            .path()
            .strip_prefix(&home)
            .expect("tempfile is under home");
        let tilde_path = format!("~/{}", relative.to_string_lossy());
        let result = resolve_prompt_file(&tilde_path, "step", None).unwrap();
        assert_eq!(result, "home content");
    }

    // ── resolve_step_system_prompts ──────────────────────────────────────────

    fn make_step_with_append(entries: Vec<SystemPromptEntry>) -> Step {
        Step {
            id: StepId("test-step".to_string()),
            body: StepBody::Prompt("say hi".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: Some(entries),
            system_prompt: None,
            resume: false,
        }
    }

    fn make_test_session() -> Session {
        let pipeline = Pipeline {
            steps: vec![],
            source: None,
            defaults: Default::default(),
            timeout_seconds: None,
            default_tools: None,
        };
        Session::new(pipeline, "test invocation".to_string())
            .with_log_provider(Box::new(NullProvider))
    }

    #[test]
    fn resolve_step_system_prompts_text_entry_returns_inline_text() {
        let step = make_step_with_append(vec![SystemPromptEntry::Text("inline text".to_string())]);
        let session = make_test_session();
        let (system_prompt, append) =
            resolve_step_system_prompts(&step, &session, "test-step", None).unwrap();
        assert!(system_prompt.is_none());
        assert_eq!(append, vec!["inline text"]);
    }

    #[test]
    fn resolve_step_system_prompts_file_entry_reads_file_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "from file").unwrap();
        let step = make_step_with_append(vec![SystemPromptEntry::File(tmp.path().to_path_buf())]);
        let session = make_test_session();
        let (_system_prompt, append) =
            resolve_step_system_prompts(&step, &session, "test-step", None).unwrap();
        assert_eq!(append, vec!["from file"]);
    }

    #[test]
    fn resolve_step_system_prompts_shell_entry_captures_stdout() {
        let step = make_step_with_append(vec![SystemPromptEntry::Shell(
            "echo 'from shell'".to_string(),
        )]);
        let session = make_test_session();
        let (_system_prompt, append) =
            resolve_step_system_prompts(&step, &session, "test-step", None).unwrap();
        assert_eq!(append.len(), 1);
        assert!(
            append[0].contains("from shell"),
            "expected shell output, got: {:?}",
            append[0]
        );
    }

    #[test]
    fn resolve_step_system_prompts_multiple_entries_all_resolved() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "file content").unwrap();
        let step = make_step_with_append(vec![
            SystemPromptEntry::Text("first".to_string()),
            SystemPromptEntry::File(tmp.path().to_path_buf()),
            SystemPromptEntry::Shell("echo 'third'".to_string()),
        ]);
        let session = make_test_session();
        let (_system_prompt, append) =
            resolve_step_system_prompts(&step, &session, "test-step", None).unwrap();
        assert_eq!(append.len(), 3);
        assert_eq!(append[0], "first");
        assert_eq!(append[1], "file content");
        assert!(
            append[2].contains("third"),
            "expected 'third' in shell output"
        );
    }

    #[test]
    fn resolve_step_system_prompts_no_entries_returns_empty() {
        let step = Step {
            id: StepId("no-append".to_string()),
            body: StepBody::Prompt("hello".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let session = make_test_session();
        let (system_prompt, append) =
            resolve_step_system_prompts(&step, &session, "no-append", None).unwrap();
        assert!(system_prompt.is_none());
        assert!(append.is_empty());
    }
}
