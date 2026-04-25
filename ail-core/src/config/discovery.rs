use std::path::{Path, PathBuf};

use super::project_state;

const AIL_DIRNAME: &str = ".ail";

/// A discovered pipeline file with a display name derived from its location
/// under `.ail/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineEntry {
    /// Display name shown in the picker. For `.ail/<sub>/default.yaml` this is
    /// just the subdir name (`<sub>`); for `.ail/<sub>/<other>.yaml` it is
    /// `<sub>/<other>` (the inner extension is stripped, including any double
    /// extension like `.ail.yaml`).
    pub name: String,
    /// Absolute path used to load the pipeline.
    pub path: PathBuf,
}

/// Outcome of pipeline discovery — see [`discover`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryResult {
    /// A single pipeline path resolved unambiguously. Caller should `load()` it.
    Resolved(PathBuf),
    /// Multiple candidate pipelines exist and no preference is recorded
    /// (no last-used pointer, no marker file). Callers in TTY contexts should
    /// present these to the user; non-TTY callers should error with the list.
    Ambiguous(Vec<PipelineEntry>),
    /// No pipeline found anywhere on the search path. Caller falls through to
    /// passthrough mode.
    None,
}

/// Locate a pipeline file using the project-aware discovery order.
///
/// Order:
///   1. Explicit `--pipeline <path>` (whatever the caller passed).
///   2. Per-project last-used pointer (`~/.ail/projects/<sha1>/last_pipeline`).
///   3. Project default marker (`<cwd>/.ail/default`).
///   4. Subdir enumeration of `<cwd>/.ail/<sub>/*.{yaml,yml}` (depth 2):
///      - exactly one candidate → Resolved
///      - multiple candidates    → Ambiguous (caller picks)
///      - zero candidates        → fall through
///   5. User-global default `~/.config/ail/default.yaml`.
///   6. Otherwise → None (passthrough).
///
/// Files deeper than depth 2 (e.g. `.ail/<sub>/agents/foo.ail.yaml`) are
/// invisible to the picker by design — they're sub-pipelines, not entry points.
/// They remain reachable via `--pipeline` or by writing their relative path
/// into the marker file.
pub fn discover(explicit: Option<PathBuf>) -> DiscoveryResult {
    if let Some(path) = explicit {
        return DiscoveryResult::Resolved(path);
    }

    if let Some(p) = project_state::read_last_used() {
        return DiscoveryResult::Resolved(p);
    }

    if let Ok(cwd) = std::env::current_dir() {
        let ail_dir = cwd.join(AIL_DIRNAME);

        if let Some(p) = project_state::read_marker(&ail_dir) {
            return DiscoveryResult::Resolved(p);
        }

        let candidates = scan_subdirs(&ail_dir);
        match candidates.len() {
            0 => {} // fall through to user-global
            1 => {
                return DiscoveryResult::Resolved(
                    candidates.into_iter().next().expect("len==1").path,
                );
            }
            _ => return DiscoveryResult::Ambiguous(candidates),
        }
    }

    if let Some(home) = dirs::home_dir() {
        let user_default = home.join(".config/ail/default.yaml");
        if user_default.exists() {
            return DiscoveryResult::Resolved(user_default);
        }
    }

    DiscoveryResult::None
}

/// List every pipeline candidate under `<cwd>/.ail/`. Includes the per-project
/// user-global default as a `~/.config/ail/<stem>.yaml` entry only if the CWD
/// has no candidates of its own. Used by callers that want to render their
/// own picker UI.
pub fn discover_all() -> Vec<PipelineEntry> {
    let mut out = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        out.extend(scan_subdirs(&cwd.join(AIL_DIRNAME)));
    }
    if out.is_empty() {
        if let Some(home) = dirs::home_dir() {
            scan_user_global(&home.join(".config/ail"), &mut out);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Walk `<ail_dir>/<sub>/*.{yaml,yml}` to depth 2 and return one entry per file.
fn scan_subdirs(ail_dir: &Path) -> Vec<PipelineEntry> {
    let mut out = Vec::new();
    let Ok(subdirs) = std::fs::read_dir(ail_dir) else {
        return out;
    };
    for entry in subdirs.flatten() {
        let sub_path = entry.path();
        if !sub_path.is_dir() {
            continue;
        }
        let Some(sub_name) = sub_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(files) = std::fs::read_dir(&sub_path) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if !p.is_file() {
                continue;
            }
            if !is_yaml(&p) {
                continue;
            }
            let stem = display_stem(&p);
            let display_name = if stem == "default" {
                sub_name.to_string()
            } else {
                format!("{sub_name}/{stem}")
            };
            out.push(PipelineEntry {
                name: display_name,
                path: p,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn scan_user_global(dir: &Path, out: &mut Vec<PipelineEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() || !is_yaml(&p) {
            continue;
        }
        let stem = display_stem(&p);
        out.push(PipelineEntry {
            name: stem,
            path: p,
        });
    }
}

fn is_yaml(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    )
}

/// Strip `.yaml`/`.yml`, then a trailing `.ail` if present, so
/// `code-review.ail.yaml` → `code-review`.
fn display_stem(p: &Path) -> String {
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    stem.strip_suffix(".ail")
        .map(str::to_string)
        .unwrap_or(stem)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Serialise all CWD-mutating tests through this mutex so they don't race.
    static TEST_CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn write_pipeline(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            "version: \"0.0.1\"\npipeline:\n  - id: x\n    prompt: hi\n",
        )
        .unwrap();
    }

    // ── discover() ───────────────────────────────────────────────────────────

    #[test]
    fn explicit_path_is_returned_as_resolved() {
        let path = PathBuf::from("/some/explicit/path.ail.yaml");
        assert_eq!(
            discover(Some(path.clone())),
            DiscoveryResult::Resolved(path)
        );
    }

    #[test]
    fn no_files_returns_none() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result = discover(None);

        std::env::set_current_dir(&original).unwrap();
        // No CWD candidates; ~/.config/ail/default.yaml may or may not exist on
        // the host. We only assert that we did NOT pick anything from the tempdir.
        if let DiscoveryResult::Resolved(p) = result {
            assert!(!p.starts_with(dir.path()));
        }
    }

    #[test]
    fn single_subdir_with_default_yaml_resolves() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        write_pipeline(&canonical.join(".ail/starter/default.yaml"));

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&canonical).unwrap();
        let result = discover(None);
        std::env::set_current_dir(&original).unwrap();

        match result {
            DiscoveryResult::Resolved(p) => {
                assert_eq!(p, canonical.join(".ail/starter/default.yaml"))
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    #[test]
    fn multiple_subdir_candidates_returns_ambiguous() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        write_pipeline(&canonical.join(".ail/starter/default.yaml"));
        write_pipeline(&canonical.join(".ail/oh-my-ail/.ohmy.ail.yaml"));

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&canonical).unwrap();
        let result = discover(None);
        std::env::set_current_dir(&original).unwrap();

        match result {
            DiscoveryResult::Ambiguous(entries) => {
                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
                // `.ohmy.ail.yaml` → stem `.ohmy.ail`, then strip `.ail` → `.ohmy`
                assert!(
                    names.contains(&"starter"),
                    "expected `starter` in {names:?}"
                );
                assert!(
                    names.iter().any(|n| n.starts_with("oh-my-ail/")),
                    "expected an oh-my-ail/* entry in {names:?}"
                );
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn marker_overrides_subdir_enumeration() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        write_pipeline(&canonical.join(".ail/starter/default.yaml"));
        write_pipeline(&canonical.join(".ail/oh-my-ail/.ohmy.ail.yaml"));
        // Even though there are two candidates, the marker forces a choice.
        std::fs::write(canonical.join(".ail/default"), "oh-my-ail/.ohmy.ail.yaml").unwrap();

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&canonical).unwrap();
        let result = discover(None);
        std::env::set_current_dir(&original).unwrap();

        match result {
            DiscoveryResult::Resolved(p) => {
                assert_eq!(p, canonical.join(".ail/oh-my-ail/.ohmy.ail.yaml"))
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    #[test]
    fn deep_subpipeline_files_are_invisible_to_picker() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        // depth-2 entry: visible
        write_pipeline(&canonical.join(".ail/oh-my-ail/.ohmy.ail.yaml"));
        // depth-3 sub-pipeline: hidden
        write_pipeline(&canonical.join(".ail/oh-my-ail/agents/atlas.ail.yaml"));

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&canonical).unwrap();
        let result = discover(None);
        std::env::set_current_dir(&original).unwrap();

        // Only one candidate at depth 2, so it resolves directly.
        match result {
            DiscoveryResult::Resolved(p) => {
                assert_eq!(p, canonical.join(".ail/oh-my-ail/.ohmy.ail.yaml"))
            }
            other => panic!("expected Resolved, got {other:?}"),
        }
    }

    // ── discover_all() ───────────────────────────────────────────────────────

    #[test]
    fn discover_all_returns_subdir_pipelines_with_display_names() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        write_pipeline(&canonical.join(".ail/starter/default.yaml"));
        write_pipeline(&canonical.join(".ail/superpowers/code-review.ail.yaml"));
        write_pipeline(&canonical.join(".ail/superpowers/brainstorming.ail.yaml"));

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&canonical).unwrap();
        let entries = discover_all();
        std::env::set_current_dir(&original).unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"starter"));
        assert!(names.contains(&"superpowers/code-review"));
        assert!(names.contains(&"superpowers/brainstorming"));
        // Sorted alphabetically.
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn discover_all_returns_empty_in_clean_cwd() {
        let _g = TEST_CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let entries = discover_all();
        std::env::set_current_dir(&original).unwrap();
        // Host's ~/.config/ail/ is the only possible source of entries.
        for e in entries {
            assert!(!e.path.starts_with(dir.path()));
        }
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    #[test]
    fn display_stem_strips_double_extension() {
        assert_eq!(
            display_stem(Path::new("code-review.ail.yaml")),
            "code-review"
        );
        assert_eq!(display_stem(Path::new("default.yaml")), "default");
        assert_eq!(display_stem(Path::new(".ohmy.ail.yaml")), ".ohmy");
    }
}
