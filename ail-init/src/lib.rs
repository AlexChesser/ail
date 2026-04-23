#![allow(clippy::result_large_err)]
//! `ail init` — scaffold an ail workspace from a bundled starter template.
//!
//! Templates live in `demo/<name>/` and each declares a `template.yaml` manifest
//! with its name, aliases, and description. Milestones 2+ fill in bundling,
//! installation, and the interactive picker.

use ail_core::error::AilError;

pub struct InitArgs {
    pub template: Option<String>,
    pub force: bool,
    pub dry_run: bool,
}

pub fn run(args: InitArgs) -> Result<(), AilError> {
    let _ = args;
    println!(
        "ail init: scaffolding coming soon on this branch. \
         Bundled templates, conflict detection, and the interactive picker \
         land in subsequent commits."
    );
    Ok(())
}

pub fn help_summary() -> &'static str {
    "Scaffold an ail workspace from a starter template"
}
