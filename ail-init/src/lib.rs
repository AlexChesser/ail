#![allow(clippy::result_large_err)]
//! `ail init` — scaffold an ail workspace from a bundled starter template.
//!
//! Templates live in `demo/<name>/` with a `template.yaml` manifest at the
//! root of each. `BundledSource` embeds them via `include_dir!` at compile
//! time so `demo/` is the single source of truth.

mod manifest;
mod source;
mod template;

use ail_core::error::AilError;
use source::{bundled::BundledSource, TemplateSource};

pub use template::{Template, TemplateFile, TemplateMeta};

pub struct InitArgs {
    pub template: Option<String>,
    pub force: bool,
    pub dry_run: bool,
}

pub fn run(args: InitArgs) -> Result<(), AilError> {
    let source = BundledSource::new()?;

    match args.template.as_deref() {
        Some(name) => {
            let template = source
                .fetch(name)
                .ok_or_else(|| template_not_found(name, &source))?;
            println!(
                "ail init: resolved `{}` ({} files). Install step lands in milestone 4.",
                template.meta.name,
                template.files.len()
            );
            if args.force {
                println!("(--force requested — honoured at install time)");
            }
            if args.dry_run {
                println!("(--dry-run requested — honoured at install time)");
            }
        }
        None => {
            println!("Available templates:");
            for meta in source.list() {
                let aliases = if meta.aliases.is_empty() {
                    String::new()
                } else {
                    format!(" (aliases: {})", meta.aliases.join(", "))
                };
                println!("  {}{} — {}", meta.name, aliases, meta.short_description);
            }
            println!();
            println!("Usage: ail init <TEMPLATE>");
            println!("Interactive picker lands in milestone 6.");
        }
    }

    Ok(())
}

pub fn help_summary() -> &'static str {
    "Scaffold an ail workspace from a starter template"
}

fn template_not_found(name: &str, source: &BundledSource) -> AilError {
    let available: Vec<String> = source.list().iter().map(|m| m.name.clone()).collect();
    AilError::config_not_found(format!(
        "template `{name}` not found. Available: {}",
        available.join(", ")
    ))
}
