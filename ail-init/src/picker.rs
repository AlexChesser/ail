#![allow(clippy::result_large_err)]

use crate::source::bundled::BundledSource;
use crate::source::TemplateSource;
use ail_core::error::AilError;

/// Present an interactive picker over bundled templates. Returns `None` if the
/// user cancelled (Esc / Ctrl-C). Callers should only invoke this when stdin
/// is a TTY; non-TTY callers should fall back to a text listing and a
/// "--template required" error.
pub(crate) fn pick(source: &BundledSource) -> Result<Option<String>, AilError> {
    let metas = source.list();
    if metas.is_empty() {
        return Ok(None);
    }

    let items: Vec<String> = metas
        .iter()
        .map(|m| {
            let aliases = if m.aliases.is_empty() {
                String::new()
            } else {
                format!(" (aliases: {})", m.aliases.join(", "))
            };
            format!("{}{} — {}", m.name, aliases, m.short_description)
        })
        .collect();

    let selection = dialoguer::Select::new()
        .with_prompt("Select a template to install")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| AilError::config_validation(format!("interactive picker failed: {e}")))?;

    Ok(selection.map(|idx| metas[idx].name.clone()))
}
