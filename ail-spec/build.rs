use glob::glob;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

struct SpecFile {
    /// Section ID: "s05", "r02", etc.
    id: String,
    /// Title extracted from first ## heading
    title: String,
    /// Word count of the file
    word_count: usize,
    /// Path relative to workspace root (for include_str!)
    rel_path: String,
    /// Absolute path used for build-time content reads.
    abs_path: PathBuf,
    /// "core" or "runner"
    category: &'static str,
    /// Section group/category for logical organization
    group: String,
}

/// Section IDs excluded from the `compact` (authoring) tier wholesale.
/// Compact is the LLM-authoring reference: it includes only sections
/// describing how to write a correct pipeline. Excluded sections cover
/// purpose/philosophy, ail's internals, MVP/roadmap/open questions,
/// operational/consumer tooling, CLI scaffolding, and plugin-author
/// material that doesn't help an LLM write a `.ail.yaml`.
const COMPACT_EXCLUDE: &[&str] = &[
    "s01", // Purpose & non-goals (philosophy)
    "s19", // Runners & adapters (ail internals)
    "s20", // MVP scope (status)
    "s21", // Planned extensions (roadmap)
    "s22", // Open questions (status)
    "s23", // Structured output mode (consumer-facing event stream)
    "s24", // log command (operational)
    "s25", // logs command (operational)
    "s31", // Specification access (meta)
    "s32", // ail init (scaffolding)
    "s33", // URL template source (scaffolding)
    "s34", // ail agent-guide (CLAUDE.md scaffolding)
    "r04", // ail log format spec (for log consumers)
    "r10", // Plugin protocol (for plugin authors)
    "r11", // Plugin discovery (for plugin authors)
];

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir).join("embedded_specs.rs");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap();

    let mut specs = Vec::new();

    for (pattern, category) in &[("spec/core/s*.md", "core"), ("spec/runner/r*.md", "runner")] {
        let full_pattern = workspace_root.join(pattern).to_string_lossy().to_string();
        let mut paths: Vec<PathBuf> = glob(&full_pattern)
            .expect("Failed to read glob pattern")
            .filter_map(Result::ok)
            .collect();
        paths.sort();

        for path in paths {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let id = stem.split('-').next().unwrap_or(&stem).to_string();

            let content = fs::read_to_string(&path).unwrap_or_default();
            let title = extract_title(&content);
            let word_count = content.split_whitespace().count();
            let rel_path = path
                .strip_prefix(workspace_root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let group = group_for_section_id(&id, category);

            specs.push(SpecFile {
                id,
                title,
                word_count,
                rel_path,
                abs_path: path.clone(),
                category,
                group,
            });
        }
    }

    let mut out = fs::File::create(&out_path).expect("Failed to create generated file");

    writeln!(out, "pub struct SpecSection {{").unwrap();
    writeln!(out, "    pub id: &'static str,").unwrap();
    writeln!(out, "    pub title: &'static str,").unwrap();
    writeln!(out, "    pub word_count: usize,").unwrap();
    writeln!(out, "    pub category: &'static str,").unwrap();
    writeln!(out, "    pub group: &'static str,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Per-section constants
    for spec in &specs {
        let const_name = spec.id.to_uppercase();
        writeln!(
            out,
            "pub const SECTION_{}: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../{}\"));",
            const_name, spec.rel_path
        )
        .unwrap();
    }
    writeln!(out).unwrap();

    // Section registry
    writeln!(out, "pub const SECTIONS: &[SpecSection] = &[").unwrap();
    for spec in &specs {
        let const_name = spec.id.to_uppercase();
        writeln!(
            out,
            "    SpecSection {{ id: \"{}\", title: \"{}\", word_count: {}, category: \"{}\", group: \"{}\" }},",
            spec.id,
            spec.title.replace('"', "\\\""),
            spec.word_count,
            spec.category,
            spec.group
        )
        .unwrap();
        // Reference the constant so it's not unused
        let _ = const_name;
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // Lookup function
    writeln!(
        out,
        "pub fn section_content(id: &str) -> Option<&'static str> {{"
    )
    .unwrap();
    writeln!(out, "    match id {{").unwrap();
    for spec in &specs {
        let const_name = spec.id.to_uppercase();
        writeln!(
            out,
            "        \"{}\" => Some(SECTION_{}),",
            spec.id, const_name
        )
        .unwrap();
    }
    writeln!(out, "        _ => None,").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Concatenated constants
    write_concatenated(&mut out, "CORE_PROSE", &specs, "core");
    write_concatenated(&mut out, "RUNNER_PROSE", &specs, "runner");
    write_concatenated_all(&mut out, "FULL_PROSE", &specs);

    // T1 schema
    writeln!(
        out,
        "pub const SCHEMA: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../spec/compressed/schema.yaml\"));"
    )
    .unwrap();

    // T2 compact — derived from authoring-relevant sections with
    // <!-- compact:skip --> ... <!-- /compact:skip --> blocks stripped.
    let compact = build_compact(&specs);
    writeln!(
        out,
        "pub const COMPACT: &str = {};",
        rust_string_literal(&compact)
    )
    .unwrap();

    // Rerun triggers
    println!("cargo:rerun-if-changed=../spec/core");
    println!("cargo:rerun-if-changed=../spec/runner");
    println!("cargo:rerun-if-changed=../spec/compressed");
}

/// Build the T2 compact reference by concatenating authoring-relevant
/// sections (filtered against `COMPACT_EXCLUDE`) with `<!-- compact:skip -->`
/// blocks stripped. Runs at build time so the result is embedded as a
/// `&'static str`.
fn build_compact(specs: &[SpecFile]) -> String {
    let mut out = String::new();
    out.push_str("# AIL Specification — Authoring Reference\n\n");
    out.push_str(
        "Everything an LLM needs to write a correct `.ail.yaml` pipeline. \
         Excludes ail's internals, roadmap, status notes, operational tooling, \
         and CLI scaffolding. Generated at build time from `spec/core/` and \
         `spec/runner/` — single source of truth, no hand-curated mirror.\n\n",
    );

    for spec in specs {
        if COMPACT_EXCLUDE.contains(&spec.id.as_str()) {
            continue;
        }
        let raw = fs::read_to_string(&spec.abs_path).unwrap_or_default();
        let stripped = strip_compact_skip(&raw);
        out.push_str(&stripped);
        if !stripped.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }

    out
}

/// Strip every `<!-- compact:skip --> ... <!-- /compact:skip -->` block
/// (including the markers themselves) from a section's prose. Tags must
/// appear on their own line. Unmatched opens are tolerated by leaving
/// the remainder of the file in place.
fn strip_compact_skip(input: &str) -> String {
    const OPEN: &str = "<!-- compact:skip -->";
    const CLOSE: &str = "<!-- /compact:skip -->";

    let mut out = String::with_capacity(input.len());
    let mut skipping = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if !skipping && trimmed == OPEN {
            skipping = true;
            continue;
        }
        if skipping && trimmed == CLOSE {
            skipping = false;
            continue;
        }
        if skipping {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    out
}

/// Emit a Rust string literal for `s` — uses a raw string with a `#`
/// guard sequence longer than any run of `#`s in the input, so any
/// content (including embedded `"`, `\`, and markdown) round-trips
/// exactly without escape processing.
fn rust_string_literal(s: &str) -> String {
    let mut hashes = 1usize;
    loop {
        let needle = format!("\"{}", "#".repeat(hashes));
        if !s.contains(&needle) {
            break;
        }
        hashes += 1;
    }
    let pad = "#".repeat(hashes);
    format!("r{pad}\"{s}\"{pad}")
}

fn extract_title(content: &str) -> String {
    // Find the first markdown heading (# or ##, etc.) in the file
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            // Skip leading # chars, then trim whitespace
            let title = trimmed.trim_start_matches('#').trim();
            // Only return if there's actual content after the #
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    String::new()
}

fn group_for_section_id(id: &str, category: &str) -> String {
    if category == "runner" {
        return "Runner Spec".to_string();
    }

    // Parse section number from id like "s05"
    if let Ok(num) = id.chars().skip(1).collect::<String>().parse::<u32>() {
        match num {
            1..=2 => "Foundation".to_string(),
            3..=5 => "Core Language".to_string(),
            6..=13 => "Pipeline Authoring".to_string(),
            14..=16 => "Runtime".to_string(),
            17..=22 => "Appendix".to_string(),
            23 | 26..=30 => "Advanced Features".to_string(),
            24..=25 | 31..=33 => "CLI Tooling".to_string(),
            _ => "Uncategorized".to_string(),
        }
    } else {
        "Uncategorized".to_string()
    }
}

fn write_concatenated(out: &mut fs::File, name: &str, specs: &[SpecFile], category: &str) {
    writeln!(out, "pub fn {}_fn() -> String {{", name.to_lowercase()).unwrap();
    writeln!(out, "    let mut s = String::new();").unwrap();
    for spec in specs.iter().filter(|s| s.category == category) {
        let const_name = spec.id.to_uppercase();
        writeln!(out, "    s.push_str(SECTION_{});", const_name).unwrap();
        writeln!(out, "    s.push_str(\"\\n\\n\");").unwrap();
    }
    writeln!(out, "    s").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
}

fn write_concatenated_all(out: &mut fs::File, name: &str, specs: &[SpecFile]) {
    writeln!(out, "pub fn {}_fn() -> String {{", name.to_lowercase()).unwrap();
    writeln!(out, "    let mut s = String::new();").unwrap();
    for spec in specs {
        let const_name = spec.id.to_uppercase();
        writeln!(out, "    s.push_str(SECTION_{});", const_name).unwrap();
        writeln!(out, "    s.push_str(\"\\n\\n\");").unwrap();
    }
    writeln!(out, "    s").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
}
