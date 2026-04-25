//! Tests for the AIL Runner Plugin Protocol (spec/runner/r10-plugin-protocol.md).
//!
//! Uses shell scripts as minimal protocol-compliant plugins to test the
//! ProtocolRunner end-to-end.

use ail_core::runner::plugin::discovery::discover_plugins_in;
use ail_core::runner::plugin::ProtocolRunner;
use ail_core::runner::{InvokeOptions, Runner};
use std::io::Write;
use std::path::Path;

/// Create a shell script that implements the minimum viable plugin protocol.
/// It reads JSON-RPC from stdin, responds to initialize and invoke, then shuts down.
fn create_echo_plugin(dir: &Path) -> std::path::PathBuf {
    let script = dir.join("echo-plugin");
    let mut f = std::fs::File::create(&script).unwrap();
    // This shell script uses a simple approach:
    // - Read lines from stdin
    // - Use basic string matching to identify methods
    // - Respond with appropriate JSON-RPC
    f.write_all(
        br#"#!/bin/sh
while IFS= read -r line; do
    case "$line" in
        *'"method":"initialize"'*)
            id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
            printf '{"jsonrpc":"2.0","id":%s,"result":{"name":"echo","version":"0.1.0","protocol_version":"1","capabilities":{"streaming":false,"session_resume":false,"tool_events":false,"permission_requests":false}}}\n' "$id"
            ;;
        *'"method":"invoke"'*)
            id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
            # Extract prompt - simplified extraction
            prompt=$(echo "$line" | sed 's/.*"prompt":"\([^"]*\)".*/\1/')
            printf '{"jsonrpc":"2.0","id":%s,"result":{"response":"Echo: %s","input_tokens":10,"output_tokens":5}}\n' "$id" "$prompt"
            ;;
        *'"method":"shutdown"'*)
            id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
            printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"
            exit 0
            ;;
    esac
done
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// Create a plugin that streams notifications before responding.
fn create_streaming_plugin(dir: &Path) -> std::path::PathBuf {
    let script = dir.join("stream-plugin");
    let mut f = std::fs::File::create(&script).unwrap();
    f.write_all(
        br#"#!/bin/sh
while IFS= read -r line; do
    case "$line" in
        *'"method":"initialize"'*)
            id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
            printf '{"jsonrpc":"2.0","id":%s,"result":{"name":"streamer","version":"0.1.0","protocol_version":"1","capabilities":{"streaming":true,"session_resume":false,"tool_events":false,"permission_requests":false}}}\n' "$id"
            ;;
        *'"method":"invoke"'*)
            id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
            # Emit streaming notifications
            printf '{"jsonrpc":"2.0","method":"stream/delta","params":{"text":"Hello "}}\n'
            printf '{"jsonrpc":"2.0","method":"stream/delta","params":{"text":"World"}}\n'
            printf '{"jsonrpc":"2.0","method":"stream/cost_update","params":{"cost_usd":0.01,"input_tokens":5,"output_tokens":2}}\n'
            # Then the final response
            printf '{"jsonrpc":"2.0","id":%s,"result":{"response":"Hello World","cost_usd":0.01,"input_tokens":5,"output_tokens":2}}\n' "$id"
            ;;
        *'"method":"shutdown"'*)
            id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
            printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"
            exit 0
            ;;
    esac
done
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

fn write_manifest(dir: &Path, name: &str, exe: &Path) {
    let manifest = dir.join(format!("{name}.yaml"));
    let mut f = std::fs::File::create(&manifest).unwrap();
    write!(
        f,
        "name: {name}\nversion: '1.0.0'\nexecutable: '{}'\nprotocol_version: '1'\n",
        exe.display()
    )
    .unwrap();
}

#[test]
fn echo_plugin_invoke_returns_response() {
    let dir = tempfile::tempdir().unwrap();
    let exe = create_echo_plugin(dir.path());
    write_manifest(dir.path(), "echo", &exe);

    let registry = discover_plugins_in(dir.path());
    let manifest = registry.get("echo").unwrap();
    let runner = ProtocolRunner::new(manifest.clone());

    let result = runner
        .invoke("hello world", InvokeOptions::default())
        .unwrap();
    assert_eq!(result.response, "Echo: hello world");
    assert_eq!(result.input_tokens, 10);
    assert_eq!(result.output_tokens, 5);
}

#[test]
fn streaming_plugin_emits_events_then_response() {
    let dir = tempfile::tempdir().unwrap();
    let exe = create_streaming_plugin(dir.path());
    write_manifest(dir.path(), "streamer", &exe);

    let registry = discover_plugins_in(dir.path());
    let manifest = registry.get("streamer").unwrap();
    let runner = ProtocolRunner::new(manifest.clone());

    let (tx, rx) = std::sync::mpsc::channel();
    let result = runner
        .invoke_streaming("test", InvokeOptions::default(), tx)
        .unwrap();

    assert_eq!(result.response, "Hello World");
    assert_eq!(result.cost_usd, Some(0.01));

    // Collect streaming events
    let events: Vec<_> = rx.try_iter().collect();
    // Should have at least the delta events, cost update, and completed
    assert!(
        events.len() >= 3,
        "expected at least 3 events, got {}",
        events.len()
    );
}

#[test]
fn plugin_runner_via_factory() {
    use ail_core::config::domain::ProviderConfig;
    use ail_core::runner::factory::RunnerFactory;
    use ail_core::runner::http::HttpSessionStore;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let dir = tempfile::tempdir().unwrap();
    let exe = create_echo_plugin(dir.path());
    write_manifest(dir.path(), "echo-test", &exe);

    let registry = discover_plugins_in(dir.path());
    let store: HttpSessionStore = Arc::new(Mutex::new(HashMap::new()));
    let provider = ProviderConfig::default();

    let runner =
        RunnerFactory::build_with_registry("echo-test", false, &store, &provider, &registry)
            .unwrap();

    let result = runner
        .invoke("factory test", InvokeOptions::default())
        .unwrap();
    assert_eq!(result.response, "Echo: factory test");
}
