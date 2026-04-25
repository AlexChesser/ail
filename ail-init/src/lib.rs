#![allow(clippy::result_large_err)]
//! `ail init` — scaffold an ail workspace from a bundled starter template.
//!
//! Templates live in `demo/<name>/` with a `template.yaml` manifest at the
//! root of each. `BundledSource` embeds them via `include_dir!` at compile
//! time so `demo/` is the single source of truth. Every template installs
//! under `$CWD/.ail/`.

mod fetcher;
mod install;
mod manifest;
mod picker;
mod source;
mod template;
mod url_ref;

use ail_core::error::AilError;
use source::{bundled::BundledSource, url::UrlSource, TemplateSource};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub use template::{Template, TemplateFile, TemplateMeta};

pub struct InitArgs {
    pub template: Option<String>,
    pub force: bool,
    pub dry_run: bool,
}

pub fn run(args: InitArgs) -> Result<(), AilError> {
    let cwd = std::env::current_dir().map_err(|e| {
        AilError::config_validation(format!("failed to resolve current directory: {e}"))
    })?;
    run_in_cwd(args, &cwd)
}

/// Testable entry point — callers pass an explicit CWD instead of relying on
/// the process-wide `current_dir`.
pub fn run_in_cwd(args: InitArgs, cwd: &Path) -> Result<(), AilError> {
    // URL-shaped args route to the URL source before we touch the bundled
    // catalog. Plain names fall through to the bundled flow below unchanged.
    if let Some(name) = args.template.as_deref() {
        if let Some(url_ref) = url_ref::parse(name)? {
            let template = UrlSource::new().fetch_url(&url_ref)?;
            return finish(&template, cwd, &args);
        }
    }

    let source = BundledSource::new()?;

    let name_owned: String;
    let name: &str = match args.template.as_deref() {
        Some(n) => n,
        None => {
            if std::io::stdin().is_terminal() {
                match picker::pick(&source)? {
                    Some(chosen) => {
                        name_owned = chosen;
                        &name_owned
                    }
                    None => {
                        println!("Cancelled.");
                        return Ok(());
                    }
                }
            } else {
                list_templates(&source);
                return Ok(());
            }
        }
    };

    let template = source
        .fetch(name)
        .ok_or_else(|| template_not_found(name, &source))?;

    finish(&template, cwd, &args)
}

fn finish(template: &Template, cwd: &Path, args: &InitArgs) -> Result<(), AilError> {
    let plan = install::plan(template, cwd);

    if args.dry_run {
        print_dry_run(template, &plan);
        return Ok(());
    }

    install::apply(&plan, args.force)?;
    print_success(template, &plan);
    Ok(())
}

pub fn help_summary() -> &'static str {
    "Scaffold an ail workspace from a starter template"
}

fn list_templates(source: &BundledSource) {
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
    println!("Usage: ail init <TEMPLATE> [--force] [--dry-run]");
}

fn template_not_found(name: &str, source: &BundledSource) -> AilError {
    let available: Vec<String> = source.list().iter().map(|m| m.name.clone()).collect();
    AilError::config_not_found(format!(
        "template `{name}` not found. Available: {}",
        available.join(", ")
    ))
}

fn print_dry_run(template: &Template, plan: &install::InstallPlan) {
    println!(
        "ail init {} (dry run) would install {} file(s) into {}:",
        template.meta.name,
        plan.files.len(),
        plan.install_root.display()
    );
    let conflict_set: std::collections::HashSet<&PathBuf> = plan.conflicts.iter().collect();
    for f in &plan.files {
        let marker = if conflict_set.contains(&f.absolute_path) {
            " (would overwrite)"
        } else {
            ""
        };
        println!(
            "  {}/{}/{}{}",
            install::INSTALL_SUBDIR,
            template.meta.name,
            f.relative_path.display(),
            marker
        );
    }
    if !plan.conflicts.is_empty() {
        println!();
        println!(
            "Note: {} existing file(s) would need --force to overwrite.",
            plan.conflicts.len()
        );
    }
}

fn print_success(template: &Template, plan: &install::InstallPlan) {
    println!(
        "Installed `{}` — {} file(s) under {}:",
        template.meta.name,
        plan.files.len(),
        plan.install_root.display()
    );
    for f in &plan.files {
        println!(
            "  {}/{}/{}",
            install::INSTALL_SUBDIR,
            template.meta.name,
            f.relative_path.display()
        );
    }
    println!();
    println!("Next: `ail \"your prompt\"` — the installed pipeline runs automatically.");
}
