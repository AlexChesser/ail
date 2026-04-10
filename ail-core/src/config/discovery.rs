use std::collections::BTreeMap;
use std::path::PathBuf;

/// Locate a pipeline file using the four-step discovery order from SPEC §3.1.
/// Returns the first path that exists, or `None` if no file is found.
/// Does not read or parse the file.
pub fn discover(explicit: Option<PathBuf>) -> Option<PathBuf> {
    // 1. Explicit path passed via --pipeline flag.
    if let Some(path) = explicit {
        return Some(path);
    }

    // 2. `.ail.yaml` in the current working directory — return absolute path so
    //    parent() in the executor always yields a usable directory (SPEC §9).
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join(".ail.yaml");
        if candidate.exists() {
            return Some(candidate);
        }

        // 3. `.ail/default.yaml` in the current working directory.
        let candidate = cwd.join(".ail/default.yaml");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 4. `~/.config/ail/default.yaml`
    if let Some(home) = dirs::home_dir() {
        let user_default = home.join(".config/ail/default.yaml");
        if user_default.exists() {
            return Some(user_default);
        }
    }

    None
}

/// A discovered pipeline file with a display name derived from its filename stem.
#[derive(Debug, Clone)]
pub struct PipelineEntry {
    /// Display name shown in the picker (filename stem, e.g. `"code-review"`).
    pub name: String,
    /// Absolute or relative path used to load the pipeline.
    pub path: PathBuf,
}

/// Discover all available pipeline files across the standard search locations.
///
/// Scan order (CWD entries take precedence over `~/.config/ail/` by name):
/// 1. `.ail.yaml` in CWD — named `"default"`
/// 2. `*.yaml` / `*.yml` in `.ail/` in CWD — named by file stem
/// 3. `*.yaml` / `*.yml` in `~/.config/ail/` — named by file stem
///
/// Entries are deduplicated by name (first occurrence wins) and sorted alphabetically.
/// Does not read or parse any files.
pub fn discover_all() -> Vec<PipelineEntry> {
    // Use a BTreeMap keyed by name for deduplication + sorted output.
    let mut by_name: BTreeMap<String, PathBuf> = BTreeMap::new();

    // 1. `.ail.yaml` in CWD.
    let cwd_ail_yaml = PathBuf::from(".ail.yaml");
    if cwd_ail_yaml.exists() {
        by_name.entry("default".to_string()).or_insert(cwd_ail_yaml);
    }

    // 2. `.ail/*.yaml` and `.ail/*.yml` in CWD.
    scan_dir(PathBuf::from(".ail"), &mut by_name);

    // 3. `~/.config/ail/*.yaml` and `~/.config/ail/*.yml`.
    if let Some(home) = dirs::home_dir() {
        scan_dir(home.join(".config/ail"), &mut by_name);
    }

    by_name
        .into_iter()
        .map(|(name, path)| PipelineEntry { name, path })
        .collect()
}

/// Scan a directory for `.yaml` / `.yml` files, inserting entries that do not
/// already exist in `map` (first-occurrence-wins deduplication).
fn scan_dir(dir: PathBuf, map: &mut BTreeMap<String, PathBuf>) {
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return, // Directory missing or unreadable — silently skip.
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            map.entry(stem.to_string()).or_insert(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Serialise all CWD-mutating tests through this mutex so they don't race.
    static TEST_CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── discover() ───────────────────────────────────────────────────────────

    #[test]
    fn explicit_path_existing_file_is_returned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("custom.ail.yaml");
        std::fs::write(&path, "").expect("write");

        let result = discover(Some(path.clone()));
        assert_eq!(result, Some(path));
    }

    #[test]
    fn explicit_path_missing_file_is_returned_as_is() {
        // discover() returns the explicit path without checking existence
        // (the load() function is responsible for producing the file-not-found error)
        let path = PathBuf::from("/does/not/exist/pipeline.ail.yaml");
        let result = discover(Some(path.clone()));
        assert_eq!(result, Some(path));
    }

    #[test]
    fn no_pipeline_files_returns_none() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let result = discover(None);

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        assert!(result.is_none());
    }

    #[test]
    fn ail_yaml_in_cwd_is_discovered() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let yaml_path = dir.path().join(".ail.yaml");
        std::fs::write(&yaml_path, "").expect("write");

        let result = discover(None);

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(".ail.yaml"));
    }

    #[test]
    fn ail_default_yaml_discovered_when_ail_yaml_absent() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let ail_dir = dir.path().join(".ail");
        std::fs::create_dir(&ail_dir).expect("mkdir");
        let default_yaml = ail_dir.join("default.yaml");
        std::fs::write(&default_yaml, "").expect("write");

        let result = discover(None);

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(".ail/default.yaml"));
    }

    #[test]
    fn ail_yaml_takes_precedence_over_ail_default_yaml() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        // Create both .ail.yaml and .ail/default.yaml
        let yaml_path = dir.path().join(".ail.yaml");
        std::fs::write(&yaml_path, "").expect("write .ail.yaml");
        let ail_dir = dir.path().join(".ail");
        std::fs::create_dir(&ail_dir).expect("mkdir");
        let default_yaml = ail_dir.join("default.yaml");
        std::fs::write(&default_yaml, "").expect("write default.yaml");

        let result = discover(None);

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(".ail.yaml"));
    }

    #[test]
    fn explicit_path_takes_precedence_over_cwd_files() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        // .ail.yaml exists in cwd too
        let yaml_path = dir.path().join(".ail.yaml");
        std::fs::write(&yaml_path, "").expect("write");
        let explicit_path = dir.path().join("explicit.ail.yaml");
        std::fs::write(&explicit_path, "").expect("write explicit");

        let result = discover(Some(explicit_path.clone()));

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        assert_eq!(result, Some(explicit_path));
    }

    // ── discover_all() ───────────────────────────────────────────────────────

    #[test]
    fn discover_all_returns_empty_when_no_files_present() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let entries = discover_all();

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        // Only system ~/.config/ail/ entries can appear; in a clean tempdir there are none
        // from CWD. We can't assert empty if user has ~/.config/ail/ files, so only check
        // that none of the returned entries point inside our tempdir.
        for entry in entries {
            assert!(
                !entry.path.starts_with(dir.path()),
                "unexpected entry from tempdir: {:?}",
                entry.path
            );
        }
    }

    #[test]
    fn discover_all_includes_ail_yaml_as_default() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        std::fs::write(dir.path().join(".ail.yaml"), "").expect("write");

        let entries = discover_all();

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"default"), "expected 'default' entry");
    }

    #[test]
    fn discover_all_includes_ail_dir_yaml_files() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let ail_dir = dir.path().join(".ail");
        std::fs::create_dir(&ail_dir).expect("mkdir");
        std::fs::write(ail_dir.join("code-review.yaml"), "").expect("write");
        std::fs::write(ail_dir.join("test-gen.yml"), "").expect("write");

        let entries = discover_all();

        std::env::set_current_dir(&original_cwd).expect("restore cwd");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"code-review"), "expected 'code-review'");
        assert!(names.contains(&"test-gen"), "expected 'test-gen'");
    }

    #[test]
    fn discover_all_deduplicates_by_name_cwd_wins() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        // Create a CWD entry named "review" under .ail/
        let ail_dir = dir.path().join(".ail");
        std::fs::create_dir(&ail_dir).expect("mkdir");
        let cwd_review = ail_dir.join("review.yaml");
        std::fs::write(&cwd_review, "# cwd version").expect("write cwd");

        let entries = discover_all();

        std::env::set_current_dir(&original_cwd).expect("restore cwd");

        // There should be exactly one entry named "review"
        let review_entries: Vec<_> = entries.iter().filter(|e| e.name == "review").collect();
        assert_eq!(
            review_entries.len(),
            1,
            "expected exactly one 'review' entry"
        );
    }

    #[test]
    fn discover_all_returns_sorted_names() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let ail_dir = dir.path().join(".ail");
        std::fs::create_dir(&ail_dir).expect("mkdir");
        std::fs::write(ail_dir.join("zebra.yaml"), "").expect("write");
        std::fs::write(ail_dir.join("alpha.yaml"), "").expect("write");
        std::fs::write(ail_dir.join("mango.yaml"), "").expect("write");

        let entries = discover_all();

        std::env::set_current_dir(&original_cwd).expect("restore cwd");

        // discover_all() uses relative paths for .ail/ entries (".ail/alpha.yaml", etc.)
        // Filter to only the entries whose names we created (alpha/mango/zebra).
        let known = ["alpha", "mango", "zebra"];
        let local_names: Vec<&str> = entries
            .iter()
            .filter(|e| known.contains(&e.name.as_str()))
            .map(|e| e.name.as_str())
            .collect();

        assert_eq!(local_names, vec!["alpha", "mango", "zebra"]);
    }

    #[test]
    fn discover_all_ignores_non_yaml_files() {
        let _guard = TEST_CWD_LOCK.lock().expect("lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");

        let ail_dir = dir.path().join(".ail");
        std::fs::create_dir(&ail_dir).expect("mkdir");
        std::fs::write(ail_dir.join("valid.yaml"), "").expect("write yaml");
        std::fs::write(ail_dir.join("ignored.txt"), "").expect("write txt");
        std::fs::write(ail_dir.join("ignored.json"), "").expect("write json");

        let entries = discover_all();

        std::env::set_current_dir(&original_cwd).expect("restore cwd");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"valid"), "expected 'valid'");
        assert!(!names.contains(&"ignored"), "unexpected 'ignored' entry");
    }
}
