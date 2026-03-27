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

    // 2. `.ail.yaml` in the current working directory.
    let cwd_ail_yaml = PathBuf::from(".ail.yaml");
    if cwd_ail_yaml.exists() {
        return Some(cwd_ail_yaml);
    }

    // 3. `.ail/default.yaml` in the current working directory.
    let cwd_default = PathBuf::from(".ail/default.yaml");
    if cwd_default.exists() {
        return Some(cwd_default);
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
