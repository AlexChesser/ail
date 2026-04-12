//! Tests for runner plugin discovery (spec/runner/r11-plugin-discovery.md).

use ail_core::runner::plugin::discovery::discover_plugins_in;
use std::io::Write;
use std::path::Path;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn create_exe(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"#!/bin/sh\necho ok").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn discover_empty_dir_returns_empty_registry() {
    let dir = tempfile::tempdir().unwrap();
    let reg = discover_plugins_in(dir.path());
    assert!(reg.is_empty());
}

#[test]
fn discover_nonexistent_dir_returns_empty_registry() {
    let reg = discover_plugins_in(Path::new("/nonexistent/path/12345"));
    assert!(reg.is_empty());
}

#[test]
fn discover_valid_yaml_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let exe = create_exe(dir.path(), "my-runner");
    write_file(
        dir.path(),
        "my-runner.yaml",
        &format!(
            "name: my-runner\nversion: '1.0.0'\nexecutable: '{}'\nprotocol_version: '1'\n",
            exe.display()
        ),
    );

    let reg = discover_plugins_in(dir.path());
    assert_eq!(reg.len(), 1);
    let plugin = reg.get("my-runner").unwrap();
    assert_eq!(plugin.name, "my-runner");
    assert_eq!(plugin.version, "1.0.0");
    assert_eq!(plugin.protocol_version, "1");
}

#[test]
fn discover_valid_json_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let exe = create_exe(dir.path(), "json-runner");
    write_file(
        dir.path(),
        "json-runner.json",
        &format!(
            r#"{{"name":"json-runner","version":"0.2.0","executable":"{}","protocol_version":"1"}}"#,
            exe.display()
        ),
    );

    let reg = discover_plugins_in(dir.path());
    assert_eq!(reg.len(), 1);
    let plugin = reg.get("json-runner").unwrap();
    assert_eq!(plugin.version, "0.2.0");
}

#[test]
fn discover_skips_invalid_manifests() {
    let dir = tempfile::tempdir().unwrap();
    // Invalid: missing required fields
    write_file(dir.path(), "bad.yaml", "foo: bar\n");
    // Invalid: not even valid YAML
    write_file(dir.path(), "worse.yaml", "{{{{not yaml\n");

    let reg = discover_plugins_in(dir.path());
    assert!(reg.is_empty());
}

#[test]
fn discover_skips_non_manifest_extensions() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "readme.md", "# not a manifest");
    write_file(dir.path(), "notes.txt", "not a manifest either");

    let reg = discover_plugins_in(dir.path());
    assert!(reg.is_empty());
}

#[test]
fn discover_rejects_builtin_name_collision() {
    let dir = tempfile::tempdir().unwrap();
    let exe = create_exe(dir.path(), "fake-claude");
    write_file(
        dir.path(),
        "claude.yaml",
        &format!(
            "name: claude\nversion: '1.0.0'\nexecutable: '{}'\n",
            exe.display()
        ),
    );

    let reg = discover_plugins_in(dir.path());
    assert!(reg.is_empty(), "built-in name 'claude' should be rejected");
}

#[test]
fn discover_multiple_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let exe_a = create_exe(dir.path(), "runner-a");
    let exe_b = create_exe(dir.path(), "runner-b");

    write_file(
        dir.path(),
        "a.yaml",
        &format!(
            "name: runner-a\nversion: '1.0.0'\nexecutable: '{}'\n",
            exe_a.display()
        ),
    );
    write_file(
        dir.path(),
        "b.yaml",
        &format!(
            "name: runner-b\nversion: '2.0.0'\nexecutable: '{}'\n",
            exe_b.display()
        ),
    );

    let reg = discover_plugins_in(dir.path());
    assert_eq!(reg.len(), 2);
    assert!(reg.get("runner-a").is_some());
    assert!(reg.get("runner-b").is_some());
}

#[test]
fn discover_relative_executable_resolved_from_manifest_dir() {
    let dir = tempfile::tempdir().unwrap();
    create_exe(dir.path(), "local-exe");
    write_file(
        dir.path(),
        "local.yaml",
        "name: local-runner\nversion: '1.0.0'\nexecutable: './local-exe'\n",
    );

    let reg = discover_plugins_in(dir.path());
    assert_eq!(reg.len(), 1);
    let plugin = reg.get("local-runner").unwrap();
    assert_eq!(plugin.executable, dir.path().join("local-exe"));
}

#[test]
fn discover_env_vars_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let exe = create_exe(dir.path(), "env-runner");
    write_file(
        dir.path(),
        "env.yaml",
        &format!(
            "name: env-runner\nversion: '1.0.0'\nexecutable: '{}'\nenv:\n  MY_KEY: my_value\n",
            exe.display()
        ),
    );

    let reg = discover_plugins_in(dir.path());
    let plugin = reg.get("env-runner").unwrap();
    assert_eq!(plugin.env.get("MY_KEY").unwrap(), "my_value");
}

#[test]
fn factory_builds_plugin_runner() {
    use ail_core::config::domain::ProviderConfig;
    use ail_core::runner::factory::RunnerFactory;
    use ail_core::runner::http::HttpSessionStore;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let dir = tempfile::tempdir().unwrap();
    let exe = create_exe(dir.path(), "test-plugin");
    write_file(
        dir.path(),
        "test-plugin.yaml",
        &format!(
            "name: test-plugin\nversion: '1.0.0'\nexecutable: '{}'\n",
            exe.display()
        ),
    );

    let registry = discover_plugins_in(dir.path());
    let store: HttpSessionStore = Arc::new(Mutex::new(HashMap::new()));
    let provider = ProviderConfig::default();

    // Should succeed — plugin found in registry
    let result =
        RunnerFactory::build_with_registry("test-plugin", false, &store, &provider, &registry);
    assert!(result.is_ok(), "plugin runner should be found in registry");

    // Should fail — unknown name not in registry
    let result =
        RunnerFactory::build_with_registry("nonexistent", false, &store, &provider, &registry);
    match result {
        Ok(_) => panic!("expected RUNNER_NOT_FOUND error"),
        Err(err) => {
            assert!(
                err.detail().contains("test-plugin"),
                "error message should list discovered plugins: {}",
                err.detail()
            );
        }
    }
}
