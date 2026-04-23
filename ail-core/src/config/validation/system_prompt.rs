//! append_system_prompt parsing — DTO to domain conversion for system prompt entries.

#![allow(clippy::result_large_err)]

use crate::config::domain::SystemPromptEntry;
use crate::config::dto::AppendSystemPromptEntryDto;
use crate::error::AilError;

use super::cfg_err;

pub(in crate::config) fn parse_append_system_prompt(
    step_id: &str,
    entries: Vec<AppendSystemPromptEntryDto>,
) -> Result<Vec<SystemPromptEntry>, AilError> {
    entries
        .into_iter()
        .enumerate()
        .map(|(i, entry)| match entry {
            AppendSystemPromptEntryDto::Text(s) => Ok(SystemPromptEntry::Text(s)),
            AppendSystemPromptEntryDto::Structured(s) => {
                let set_count = [
                    s.text.is_some(),
                    s.file.is_some(),
                    s.shell.is_some(),
                    s.spec.is_some(),
                ]
                .iter()
                .filter(|&&b| b)
                .count();
                if set_count != 1 {
                    return Err(cfg_err!(
                        "Step '{step_id}' append_system_prompt entry {i} must have exactly one \
                         key (text, file, shell, or spec); found {set_count}"
                    ));
                }
                if let Some(text) = s.text {
                    Ok(SystemPromptEntry::Text(text))
                } else if let Some(file) = s.file {
                    Ok(SystemPromptEntry::File(std::path::PathBuf::from(file)))
                } else if let Some(shell) = s.shell {
                    Ok(SystemPromptEntry::Shell(shell))
                } else {
                    Ok(SystemPromptEntry::Spec(s.spec.expect("set_count == 1")))
                }
            }
        })
        .collect()
}
