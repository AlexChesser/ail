//! DTO → Domain validation for runner plugin manifests.

#![allow(clippy::result_large_err)]

use std::path::{Path, PathBuf};

use crate::error::AilError;

use super::manifest::PluginManifest;
use super::manifest_dto::ManifestDto;

/// Validate a raw manifest DTO into a domain `PluginManifest`.
///
/// `manifest_path` is the path to the YAML/JSON file the DTO was read from.
/// It is used for resolving relative executable paths and diagnostics.
pub fn validate(dto: ManifestDto, manifest_path: &Path) -> Result<PluginManifest, AilError> {
    let name = dto.name.filter(|s| !s.is_empty()).ok_or_else(|| {
        AilError::plugin_manifest_invalid(format!(
            "{}: 'name' field is required",
            manifest_path.display()
        ))
    })?;

    // Runner names must be alphanumeric + hyphens, and must not collide with built-ins.
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AilError::plugin_manifest_invalid(format!(
            "{}: runner name '{}' contains invalid characters (only alphanumeric, hyphens, underscores allowed)",
            manifest_path.display(),
            name
        )));
    }

    let builtin_names = ["claude", "http", "ollama", "stub"];
    if builtin_names.contains(&name.to_lowercase().as_str()) {
        return Err(AilError::plugin_manifest_invalid(format!(
            "{}: runner name '{}' conflicts with a built-in runner",
            manifest_path.display(),
            name
        )));
    }

    let version = dto.version.filter(|s| !s.is_empty()).ok_or_else(|| {
        AilError::plugin_manifest_invalid(format!(
            "{}: 'version' field is required",
            manifest_path.display()
        ))
    })?;

    let protocol_version = dto
        .protocol_version
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "1".to_string());

    if protocol_version != "1" {
        return Err(AilError::plugin_manifest_invalid(format!(
            "{}: unsupported protocol_version '{}' (only '1' is supported)",
            manifest_path.display(),
            protocol_version
        )));
    }

    let executable_str = dto.executable.filter(|s| !s.is_empty()).ok_or_else(|| {
        AilError::plugin_manifest_invalid(format!(
            "{}: 'executable' field is required",
            manifest_path.display()
        ))
    })?;

    let executable = resolve_executable(&executable_str, manifest_path)?;

    Ok(PluginManifest {
        name,
        version,
        executable,
        protocol_version,
        env: dto.env,
        args: dto.args,
        manifest_path: manifest_path.to_path_buf(),
    })
}

/// Resolve the executable path from the manifest.
///
/// Resolution order:
/// 1. If absolute path → use as-is (must exist)
/// 2. If starts with `./` or `../` → relative to manifest directory (must exist)
/// 3. Otherwise → bare name, looked up on PATH via `which`
fn resolve_executable(raw: &str, manifest_path: &Path) -> Result<PathBuf, AilError> {
    let path = Path::new(raw);

    if path.is_absolute() {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(AilError::plugin_manifest_invalid(format!(
            "{}: executable '{}' not found",
            manifest_path.display(),
            raw
        )));
    }

    // Relative to manifest directory
    if raw.starts_with("./") || raw.starts_with("../") {
        if let Some(dir) = manifest_path.parent() {
            let resolved = dir.join(path);
            if resolved.exists() {
                return Ok(resolved);
            }
            return Err(AilError::plugin_manifest_invalid(format!(
                "{}: executable '{}' not found (resolved to '{}')",
                manifest_path.display(),
                raw,
                resolved.display()
            )));
        }
    }

    // Bare name → look up on PATH
    which_lookup(raw, manifest_path)
}

/// Look up a bare executable name on the system PATH.
fn which_lookup(name: &str, manifest_path: &Path) -> Result<PathBuf, AilError> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AilError::plugin_manifest_invalid(format!(
        "{}: executable '{}' not found on PATH",
        manifest_path.display(),
        name
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;

    fn make_dto(name: &str, executable: &str) -> ManifestDto {
        ManifestDto {
            name: Some(name.to_string()),
            version: Some("1.0.0".to_string()),
            executable: Some(executable.to_string()),
            protocol_version: Some("1".to_string()),
            env: HashMap::new(),
            args: vec![],
        }
    }

    #[test]
    fn missing_name_is_error() {
        let dto = ManifestDto {
            name: None,
            version: Some("1.0.0".to_string()),
            executable: Some("/bin/true".to_string()),
            protocol_version: Some("1".to_string()),
            env: HashMap::new(),
            args: vec![],
        };
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("name"));
    }

    #[test]
    fn empty_name_is_error() {
        let dto = make_dto("", "/bin/true");
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("name"));
    }

    #[test]
    fn builtin_name_is_rejected() {
        let dto = make_dto("claude", "/bin/true");
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("built-in"));
    }

    #[test]
    fn invalid_chars_in_name_rejected() {
        let dto = make_dto("my runner!", "/bin/true");
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("invalid characters"));
    }

    #[test]
    fn hyphen_and_underscore_allowed_in_name() {
        let dto = make_dto("my-runner_v2", "/bin/true");
        let result = validate(dto, Path::new("/tmp/test.yaml"));
        // May fail on executable not found, but name validation should pass
        if let Err(e) = &result {
            assert!(
                !e.detail().contains("invalid characters"),
                "hyphens and underscores should be allowed"
            );
        }
    }

    #[test]
    fn missing_executable_is_error() {
        let dto = ManifestDto {
            name: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            executable: None,
            protocol_version: Some("1".to_string()),
            env: HashMap::new(),
            args: vec![],
        };
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("executable"));
    }

    #[test]
    fn unsupported_protocol_version_rejected() {
        let dto = ManifestDto {
            name: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            executable: Some("/bin/true".to_string()),
            protocol_version: Some("99".to_string()),
            env: HashMap::new(),
            args: vec![],
        };
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("protocol_version"));
    }

    #[test]
    fn absolute_executable_that_exists_succeeds() {
        let dto = make_dto("test-runner", "/bin/true");
        let result = validate(dto, Path::new("/tmp/test.yaml"));
        // /bin/true exists on Linux
        if Path::new("/bin/true").exists() {
            let manifest = result.unwrap();
            assert_eq!(manifest.name, "test-runner");
            assert_eq!(manifest.executable, PathBuf::from("/bin/true"));
        }
    }

    #[test]
    fn absolute_executable_that_does_not_exist_is_error() {
        let dto = make_dto("test-runner", "/nonexistent/binary");
        let err = validate(dto, Path::new("/tmp/test.yaml")).unwrap_err();
        assert!(err.detail().contains("not found"));
    }

    #[test]
    fn relative_executable_resolved_from_manifest_dir() {
        // Create a temp executable
        let dir = tempfile::tempdir().unwrap();
        let exe_path = dir.path().join("my-runner");
        let mut f = std::fs::File::create(&exe_path).unwrap();
        f.write_all(b"#!/bin/sh\necho ok").unwrap();

        let manifest_path = dir.path().join("runner.yaml");
        let dto = make_dto("test-runner", "./my-runner");
        let manifest = validate(dto, &manifest_path).unwrap();
        assert_eq!(manifest.executable, exe_path);
    }

    #[test]
    fn default_protocol_version_is_1() {
        let dto = ManifestDto {
            name: Some("test-runner".to_string()),
            version: Some("1.0.0".to_string()),
            executable: Some("/bin/true".to_string()),
            protocol_version: None,
            env: HashMap::new(),
            args: vec![],
        };
        if Path::new("/bin/true").exists() {
            let manifest = validate(dto, Path::new("/tmp/test.yaml")).unwrap();
            assert_eq!(manifest.protocol_version, "1");
        }
    }
}
