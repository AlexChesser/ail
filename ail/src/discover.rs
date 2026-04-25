//! UI-aware wrapper around `ail_core::config::discovery`.
//!
//! `ail-core` is intentionally headless: it returns `DiscoveryResult::Ambiguous`
//! and lets the caller decide what to do with multiple candidates. This module
//! adds the binary-only behaviours: an interactive picker (TTY) and a typed
//! error listing (non-TTY).

use std::io::IsTerminal;
use std::path::PathBuf;

use ail_core::config::discovery::{discover, DiscoveryResult, PipelineEntry};

/// Resolve the discovery result to a single path, prompting the user if there
/// are multiple candidates and we're attached to a TTY. Returns `None` for the
/// passthrough case (no pipeline found anywhere). On non-TTY ambiguity, prints
/// a candidate list to stderr and exits 1 — there is no safe default.
pub fn resolve_or_passthrough(explicit: Option<PathBuf>) -> Option<PathBuf> {
    match discover(explicit) {
        DiscoveryResult::Resolved(p) => Some(p),
        DiscoveryResult::None => None,
        DiscoveryResult::Ambiguous(entries) => Some(resolve_ambiguous(entries)),
    }
}

fn resolve_ambiguous(entries: Vec<PipelineEntry>) -> PathBuf {
    if std::io::stdin().is_terminal() {
        match pick(&entries) {
            Some(idx) => {
                entries
                    .into_iter()
                    .nth(idx)
                    .expect("dialoguer index in range")
                    .path
            }
            None => {
                // User cancelled the picker.
                eprintln!("Cancelled.");
                std::process::exit(0);
            }
        }
    } else {
        eprintln!(
            "Multiple pipelines found in .ail/. Pick one with --pipeline <path>, \
             or set a default by writing the relative path to .ail/default. Candidates:"
        );
        for e in &entries {
            eprintln!("  {}  ({})", e.name, e.path.display());
        }
        std::process::exit(1);
    }
}

fn pick(entries: &[PipelineEntry]) -> Option<usize> {
    let items: Vec<String> = entries
        .iter()
        .map(|e| format!("{}  ({})", e.name, e.path.display()))
        .collect();
    dialoguer::Select::new()
        .with_prompt("Select a pipeline to run")
        .items(&items)
        .default(0)
        .interact_opt()
        .ok()
        .flatten()
}
