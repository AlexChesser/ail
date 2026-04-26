//! SPEC §31 — `ail spec --format compact` is the LLM-authoring tier.
//!
//! These tests guard the build-time-derived `compact` artifact:
//! - excluded sections stay excluded
//! - `<!-- compact:skip -->` blocks are stripped
//! - every embedded YAML example whose first non-blank line is the
//!   marker comment `# spec:validate` actually parses through
//!   `ail_core::config::load`, so a broken pinned example fails the
//!   build instead of silently shipping.
//!
//! Plain ```yaml blocks (including the existing aspirational examples
//! in §18) are documentation only — they are included verbatim in
//! compact but are not validated. The plain ```yaml fence keeps
//! GitHub's YAML syntax highlighting on the published spec; the
//! `# spec:validate` marker is a real YAML comment, harmless to the
//! loader, and serves as the explicit opt-in to CI validation.

use ail_core::config::load;
use std::path::{Path, PathBuf};

/// Section heading lines (verbatim) that must NOT appear in compact.
/// Tracks `COMPACT_EXCLUDE` in `ail-spec/build.rs`. Using the full
/// heading is robust against cross-references in body text mentioning
/// excluded section titles by name.
const EXCLUDED_HEADINGS: &[&str] = &[
    "## 1. Purpose & Philosophy",
    "## 20. MVP — v0.0.1 Scope",
    "## 21. Planned Extensions",
    "## 22. Open Questions",
    "## 23. Structured Output Mode",
    "## 24. The `ail log` Command",
    "## 25. The `ail logs` Command (Plural)",
    "## 31. Specification Access & Injection",
    "## 32. The `ail init` Command",
    "## 33. URL-Based Template Source",
    "## r04. AIL Log Format Specification",
    "## r10. AIL Runner Plugin Protocol",
    "## r11. Runner Plugin Discovery",
];

/// Substrings that must appear in compact — at least one heading from
/// every authoring-relevant section group.
const REQUIRED_SECTION_MARKERS: &[&str] = &[
    "Concepts & Vocabulary",      // s02
    "File Format",                // s03
    "Pipeline Execution Model",   // s04
    "Step Specification",         // s05
    "Skills",                     // s06
    "Pipeline Inheritance",       // s07
    "Template Variables",         // s11
    "Conditions",                 // s12
    "Human-in-the-Loop",          // s13
    "Complete Examples",          // s18
    "do_while",                   // s27
    "for_each",                   // s28
    "Parallel Step Execution",    // s29
    "Sampling Parameter Control", // s30
];

#[test]
fn compact_is_non_empty_and_starts_with_authoring_header() {
    let compact = ail_spec::compact();
    assert!(
        compact.len() > 5_000,
        "compact unexpectedly short: {} bytes",
        compact.len()
    );
    assert!(
        compact.starts_with("# AIL Specification — Authoring Reference"),
        "compact must start with the generated authoring header — got: {:?}",
        &compact[..compact.len().min(80)]
    );
}

#[test]
fn compact_strips_skip_blocks_and_their_markers() {
    let compact = ail_spec::compact();
    assert!(
        !compact.contains("<!-- compact:skip -->"),
        "compact must not contain unstripped open skip markers"
    );
    assert!(
        !compact.contains("<!-- /compact:skip -->"),
        "compact must not contain unstripped close skip markers"
    );
    // §4.5 (Controlled Execution Mode) is wrapped in a skip block in
    // its entirety; the "ExecutionControl" type name is a good sentinel
    // because it appears nowhere else in the included spec.
    assert!(
        !compact.contains("ExecutionControl"),
        "compact must not include §4.5 Controlled Execution Mode internals"
    );
}

#[test]
fn compact_excludes_every_non_authoring_section_heading() {
    let compact = ail_spec::compact();
    for heading in EXCLUDED_HEADINGS {
        assert!(
            !compact.contains(heading),
            "compact must not include excluded section heading: {heading:?}"
        );
    }
}

#[test]
fn compact_includes_every_authoring_relevant_section() {
    let compact = ail_spec::compact();
    for marker in REQUIRED_SECTION_MARKERS {
        assert!(
            compact.contains(marker),
            "compact must include a section matching marker: {marker:?}"
        );
    }
}

/// Walk every spec file (core + runner), extract YAML blocks whose
/// first non-blank line is the marker comment `# spec:validate`, and
/// load each one through `ail_core::config::load`. Plain ```yaml
/// blocks without the marker are documentation only; add the marker
/// as the first line of a block to opt it into this CI check.
#[test]
fn pinned_pipeline_examples_validate_cleanly() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let mut total = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (id, abs_path) in walk_spec_section_files(&workspace_root) {
        let Ok(content) = std::fs::read_to_string(&abs_path) else {
            continue;
        };

        for (block_index, block) in extract_validatable_yaml_blocks(&content)
            .into_iter()
            .enumerate()
        {
            total += 1;
            let tmp = tempfile::NamedTempFile::new().expect("tempfile");
            let tmp_path = tmp.path().with_extension("ail.yaml");
            std::fs::write(&tmp_path, &block).expect("write tmp pipeline");
            if let Err(err) = load(&tmp_path) {
                failures.push(format!(
                    "{id} (block #{block_index}): {err}\n--- yaml ---\n{block}\n--- end ---"
                ));
            }
            let _ = std::fs::remove_file(&tmp_path);
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} pinned pipeline example(s) failed to validate:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Yield (section_id, absolute_path) for every spec section file in
/// `spec/core/` and `spec/runner/`.
fn walk_spec_section_files(workspace_root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for dir in ["spec/core", "spec/runner"] {
        let Ok(entries) = std::fs::read_dir(workspace_root.join(dir)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".md") {
                continue;
            }
            let id = name.split('-').next().unwrap_or(&name).to_string();
            if !id.starts_with('s') && !id.starts_with('r') {
                continue;
            }
            out.push((id, entry.path()));
        }
    }
    out.sort();
    out
}

/// Pull every ```yaml fenced block whose first non-blank line is
/// the marker comment `# spec:validate`. The marker is the explicit
/// opt-in for CI validation; un-marked yaml blocks are documentation
/// only.
fn extract_validatable_yaml_blocks(markdown: &str) -> Vec<String> {
    const MARKER: &str = "# spec:validate";

    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_yaml = false;

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if !in_yaml {
            if trimmed.starts_with("```yaml") || trimmed == "```yml" {
                in_yaml = true;
                buf.clear();
            }
            continue;
        }

        if trimmed.starts_with("```") {
            in_yaml = false;
            if first_nonblank_line(&buf).map(str::trim) == Some(MARKER) {
                out.push(buf.clone());
            }
            buf.clear();
            continue;
        }

        buf.push_str(line);
        buf.push('\n');
    }

    out
}

fn first_nonblank_line(s: &str) -> Option<&str> {
    s.lines().find(|l| !l.trim().is_empty())
}
