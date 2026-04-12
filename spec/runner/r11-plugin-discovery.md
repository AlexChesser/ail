# r11. Runner Plugin Discovery

> **Status:** alpha

---

## Purpose

This document defines how ail discovers and loads runner plugins at startup. Plugins are standalone executables that speak the AIL Runner Plugin Protocol (see `r10-plugin-protocol.md`). Discovery is based on manifest files placed in a well-known directory.

---

## Discovery Directory

Plugins are discovered from:

```
~/.ail/runners/
```

ail scans this directory for manifest files at startup. If the directory does not exist, discovery completes with zero plugins (no error).

---

## Manifest Format

A manifest is a YAML or JSON file in the discovery directory. File extension must be `.yaml`, `.yml`, or `.json`.

### Required Fields

| Field | Type | Description |
|---|---|---|
| `name` | string | Runner name used in pipeline YAML (`runner: <name>`). Must be alphanumeric, hyphens, and underscores only. Must not collide with built-in runner names. |
| `version` | string | Version of the runner extension (informational) |
| `executable` | string | Path to the runner executable (see resolution rules below) |

### Optional Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `protocol_version` | string | `"1"` | Protocol version the runner speaks |
| `env` | map | `{}` | Environment variables to pass to the runner subprocess |
| `args` | array | `[]` | Command-line arguments to pass to the runner executable |

### Example (YAML)

```yaml
# ~/.ail/runners/codex.yaml
name: codex
version: "0.1.0"
executable: codex-ail-runner
protocol_version: "1"
env:
  CODEX_API_KEY: "${CODEX_API_KEY}"
args:
  - "--verbose"
```

### Example (JSON)

```json
{
  "name": "codex",
  "version": "0.1.0",
  "executable": "/usr/local/bin/codex-ail-runner",
  "protocol_version": "1"
}
```

---

## Executable Resolution

The `executable` field is resolved in this order:

1. **Absolute path** — used as-is. Must exist on disk.
2. **Relative path** (starts with `./` or `../`) — resolved relative to the manifest file's directory. Must exist on disk.
3. **Bare name** — looked up on the system `PATH`.

If the executable cannot be found, the manifest is invalid and the plugin is skipped with a warning.

---

## Runner Name Rules

- Must contain only ASCII alphanumeric characters, hyphens (`-`), and underscores (`_`).
- Must not collide with built-in runner names: `claude`, `http`, `ollama`, `stub`.
- Names are case-sensitive — `Codex` and `codex` are different names.
- If two manifests declare the same name, first-seen wins and the duplicate is skipped with a warning.

---

## Discovery Behaviour

1. ail reads all files in `~/.ail/runners/` with `.yaml`, `.yml`, or `.json` extensions.
2. Each file is parsed and validated independently.
3. Invalid manifests are logged as warnings and skipped — they do not prevent other plugins from loading.
4. Duplicate runner names are logged as warnings — first-seen wins.
5. The resulting plugin registry is passed to `RunnerFactory` for the lifetime of the process.

Discovery happens once at process startup. Plugins cannot be hot-reloaded during a running session.

---

## Runner Selection Hierarchy

When a step specifies `runner: <name>`, the resolution order is:

1. **Built-in runners** — `claude`, `http`/`ollama`, `stub`
2. **Plugin registry** — discovered plugins from `~/.ail/runners/`
3. **Error** — `RUNNER_NOT_FOUND` with the list of known runner names

Built-in runners always take precedence over plugins with the same name (though name collisions are rejected during discovery validation).

---

## Integration with RunnerFactory

`RunnerFactory::build_with_registry()` accepts a `PluginRegistry` parameter:

```rust
RunnerFactory::build_with_registry(
    "codex",           // runner name
    false,             // headless
    &http_store,       // HTTP session store
    &provider,         // provider config
    &plugin_registry,  // discovered plugins
)
```

When the name doesn't match a built-in, the factory checks the registry and constructs a `ProtocolRunner` for the matching manifest.

The original `RunnerFactory::build()` method (without registry) continues to work for backward compatibility — it uses an empty registry.

---

## Error Handling

| Error Type | When |
|---|---|
| `ail:plugin/manifest-invalid` | Manifest fails validation (missing fields, bad name, executable not found, unsupported protocol version) |
| `ail:plugin/spawn-failed` | Plugin executable could not be started |
| `ail:plugin/protocol-error` | Plugin sent invalid JSON-RPC, unexpected response, or protocol violation |
| `ail:plugin/timeout` | Plugin did not respond within the expected timeframe |

Discovery errors (invalid manifests, missing directory) are warnings, not fatal errors. Pipeline execution can always proceed with built-in runners.
