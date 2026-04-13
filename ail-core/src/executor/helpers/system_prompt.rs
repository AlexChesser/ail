//! System prompt resolution and file loading for pipeline steps.

#![allow(clippy::result_large_err)]

use crate::config::domain::{Step, SystemPromptEntry};
use crate::error::AilError;
use crate::session::Session;
use crate::template;

use super::shell::run_shell_command;

/// Resolve `system_prompt` and all `append_system_prompt` entries for a step.
///
/// Returns `(resolved_system_prompt, resolved_append_entries)` on success.
/// On error, returns an `AilError` with step context already populated — the caller
/// may wrap with event emission before propagating.
pub(in crate::executor) fn resolve_step_system_prompts(
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
pub(in crate::executor) fn resolve_prompt_file(
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
    use crate::config::domain::{Pipeline, Step, StepBody, StepId, SystemPromptEntry};
    use crate::session::log_provider::NullProvider;
    use crate::session::Session;

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
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
        }
    }

    fn make_test_session() -> Session {
        let pipeline = Pipeline {
            steps: vec![],
            source: None,
            defaults: Default::default(),
            timeout_seconds: None,
            default_tools: None,
            named_pipelines: Default::default(),
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
            on_error: None,
            before: vec![],
            then: vec![],
            output_schema: None,
        };
        let session = make_test_session();
        let (system_prompt, append) =
            resolve_step_system_prompts(&step, &session, "no-append", None).unwrap();
        assert!(system_prompt.is_none());
        assert!(append.is_empty());
    }
}
