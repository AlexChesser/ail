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
