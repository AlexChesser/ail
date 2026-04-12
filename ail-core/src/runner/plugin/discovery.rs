//! Plugin discovery — scans `~/.ail/runners/` for runner manifests.

#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use super::manifest::PluginManifest;
use super::manifest_dto::ManifestDto;
use super::validation;
use crate::error::AilError;

/// Registry of discovered runner plugins, keyed by runner name.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, PluginManifest>,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Look up a plugin by runner name.
    pub fn get(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.get(name)
    }

    /// Number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether the registry has no plugins.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Iterate over all registered plugins.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &PluginManifest)> {
        self.plugins.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// List all registered runner names.
    pub fn runner_names(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }
}

/// Discover all runner plugins from the default directory (`~/.ail/runners/`).
///
/// Invalid manifests are logged as warnings and skipped — discovery never fails
/// unless the directory itself cannot be read. If the directory does not exist,
/// returns an empty registry.
pub fn discover_plugins() -> PluginRegistry {
    let dir = match default_plugin_dir() {
        Some(d) => d,
        None => {
            debug!("plugin discovery: could not determine ~/.ail/runners/ path");
            return PluginRegistry::empty();
        }
    };
    discover_plugins_in(&dir)
}

/// Discover runner plugins from a specific directory.
///
/// This is the testable entry point — `discover_plugins()` delegates to this
/// with the default `~/.ail/runners/` path.
pub fn discover_plugins_in(dir: &Path) -> PluginRegistry {
    let mut registry = PluginRegistry::empty();

    if !dir.exists() {
        debug!(path = %dir.display(), "plugin directory does not exist — no plugins loaded");
        return registry;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            warn!(path = %dir.display(), error = %err, "failed to read plugin directory");
            return registry;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!(error = %err, "failed to read directory entry");
                continue;
            }
        };

        let path = entry.path();
        if !is_manifest_file(&path) {
            continue;
        }

        match load_manifest(&path) {
            Ok(manifest) => {
                let name = manifest.name.clone();
                if registry.plugins.contains_key(&name) {
                    warn!(
                        name = %name,
                        path = %path.display(),
                        "duplicate runner name — skipping (first-seen wins)"
                    );
                    continue;
                }
                debug!(name = %name, path = %path.display(), "discovered runner plugin");
                registry.plugins.insert(name, manifest);
            }
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err.detail(),
                    "skipping invalid runner manifest"
                );
            }
        }
    }

    debug!(count = registry.len(), "plugin discovery complete");
    registry
}

/// Load and validate a single manifest file.
fn load_manifest(path: &Path) -> Result<PluginManifest, AilError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|err| AilError::plugin_manifest_invalid(format!("{}: {}", path.display(), err)))?;

    let dto: ManifestDto = if is_json_file(path) {
        serde_json::from_str(&contents).map_err(|err| {
            AilError::plugin_manifest_invalid(format!("{}: {}", path.display(), err))
        })?
    } else {
        serde_yaml::from_str(&contents).map_err(|err| {
            AilError::plugin_manifest_invalid(format!("{}: {}", path.display(), err))
        })?
    };

    validation::validate(dto, path)
}

fn is_manifest_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml" | "yml" | "json")
    )
}

fn is_json_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("json")
}

fn default_plugin_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ail").join("runners"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_manifest(dir: &Path, filename: &str, content: &str) {
        let path = dir.join(filename);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    fn create_dummy_exe(dir: &Path, name: &str) -> PathBuf {
        let exe = dir.join(name);
        let mut f = std::fs::File::create(&exe).unwrap();
        f.write_all(b"#!/bin/sh\necho ok").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        exe
    }

    #[test]
    fn nonexistent_directory_returns_empty_registry() {
        let registry = discover_plugins_in(Path::new("/nonexistent/path"));
        assert!(registry.is_empty());
    }

    #[test]
    fn empty_directory_returns_empty_registry() {
        let dir = tempfile::tempdir().unwrap();
        let registry = discover_plugins_in(dir.path());
        assert!(registry.is_empty());
    }

    #[test]
    fn discovers_valid_yaml_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let exe = create_dummy_exe(dir.path(), "my-runner-exe");
        write_manifest(
            dir.path(),
            "my-runner.yaml",
            &format!(
                "name: my-runner\nversion: '1.0.0'\nexecutable: '{}'\nprotocol_version: '1'\n",
                exe.display()
            ),
        );

        let registry = discover_plugins_in(dir.path());
        assert_eq!(registry.len(), 1);
        let plugin = registry.get("my-runner").unwrap();
        assert_eq!(plugin.name, "my-runner");
        assert_eq!(plugin.version, "1.0.0");
    }

    #[test]
    fn discovers_valid_json_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let exe = create_dummy_exe(dir.path(), "json-runner");
        write_manifest(
            dir.path(),
            "json-runner.json",
            &format!(
                r#"{{"name":"json-runner","version":"0.1.0","executable":"{}","protocol_version":"1"}}"#,
                exe.display()
            ),
        );

        let registry = discover_plugins_in(dir.path());
        assert_eq!(registry.len(), 1);
        assert!(registry.get("json-runner").is_some());
    }

    #[test]
    fn skips_non_manifest_files() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "readme.txt", "not a manifest");
        write_manifest(dir.path(), "notes.md", "# notes");

        let registry = discover_plugins_in(dir.path());
        assert!(registry.is_empty());
    }

    #[test]
    fn skips_invalid_manifests_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        // Invalid YAML (missing required fields)
        write_manifest(dir.path(), "bad.yaml", "not_a_manifest: true\n");

        let registry = discover_plugins_in(dir.path());
        assert!(registry.is_empty());
    }

    #[test]
    fn duplicate_names_first_wins() {
        let dir = tempfile::tempdir().unwrap();
        let exe = create_dummy_exe(dir.path(), "dupe-runner");

        // Two manifests with the same runner name but different filenames
        let content = format!(
            "name: dupe-runner\nversion: '1.0.0'\nexecutable: '{}'\n",
            exe.display()
        );
        write_manifest(dir.path(), "a-first.yaml", &content);
        write_manifest(dir.path(), "b-second.yaml", &content);

        let registry = discover_plugins_in(dir.path());
        // Only one should be registered
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn runner_names_returns_all_names() {
        let dir = tempfile::tempdir().unwrap();
        let exe1 = create_dummy_exe(dir.path(), "runner-a");
        let exe2 = create_dummy_exe(dir.path(), "runner-b");

        write_manifest(
            dir.path(),
            "a.yaml",
            &format!(
                "name: runner-a\nversion: '1.0.0'\nexecutable: '{}'\n",
                exe1.display()
            ),
        );
        write_manifest(
            dir.path(),
            "b.yaml",
            &format!(
                "name: runner-b\nversion: '1.0.0'\nexecutable: '{}'\n",
                exe2.display()
            ),
        );

        let registry = discover_plugins_in(dir.path());
        let mut names = registry.runner_names();
        names.sort();
        assert_eq!(names, vec!["runner-a", "runner-b"]);
    }

    #[test]
    fn relative_executable_resolved_from_manifest_dir() {
        let dir = tempfile::tempdir().unwrap();
        create_dummy_exe(dir.path(), "local-runner");
        write_manifest(
            dir.path(),
            "local.yaml",
            "name: local-runner\nversion: '1.0.0'\nexecutable: './local-runner'\n",
        );

        let registry = discover_plugins_in(dir.path());
        assert_eq!(registry.len(), 1);
        let plugin = registry.get("local-runner").unwrap();
        assert_eq!(plugin.executable, dir.path().join("local-runner"));
    }
}
