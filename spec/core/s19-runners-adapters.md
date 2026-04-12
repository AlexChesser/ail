## 19. Runners & Adapters

> **Implementation status:** v0.2 — three built-in runners (`claude`, `http`/`ollama`, `stub`) plus a runtime plugin system for third-party runners via the AIL Runner Plugin Protocol (JSON-RPC 2.0 over stdin/stdout).

### What a Runner Is

A runner is the underlying agent or LLM that `ail` wraps. It receives the human's prompt, produces a response, and signals completion. `ail` orchestrates everything that happens after that signal fires.

The runner is deliberately outside the pipeline language. This spec defines what pipelines do. The runner is what the pipeline acts upon.

### Three Tiers of Runner Support

**Tier 1 — Built-in runners**
Runners shipped with `ail` and maintained by the core team. Tested against every release. Their behaviour, output format, and error codes are fully understood and handled.

| Name | Type | Description |
|---|---|---|
| `claude` | `ClaudeCliRunner` | Shells out to the `claude` binary; supports streaming, session resume, tool permissions, headless bypass |
| `http` / `ollama` | `HttpRunner` | Direct OpenAI-compatible HTTP API; supports session continuity (in-memory), configurable timeouts, system prompt control |
| `stub` | `StubRunner` | Returns a fixed response; for tests and development |

**Tier 2 — Plugin runners (runtime-discoverable)**
Any executable that implements the AIL Runner Plugin Protocol. Users install plugins by placing a manifest file in `~/.ail/runners/` alongside the plugin binary. No recompilation required.

See `spec/runner/r10-plugin-protocol.md` for the protocol specification and `spec/runner/r11-plugin-discovery.md` for manifest format and discovery rules.

**Tier 3 — Custom Rust runners (compile-time)**
Community or private runners implemented as Rust crates that implement the `Runner` trait directly. These require recompilation and adding a match arm in `RunnerFactory`. Use this tier when the JSON-RPC protocol is insufficient (e.g., for runners with complex subprocess lifecycle needs).

### RunnerFactory and Per-Step Dispatch

`RunnerFactory` (`ail_core::runner::factory`) is the canonical way to obtain a runner by name at runtime. It is used by the executor for per-step runner dispatch and by the binary entry points to build the default runner.

```rust
pub struct RunnerFactory;

impl RunnerFactory {
    /// Build a runner by name, checking the plugin registry for unknown names.
    pub fn build_with_registry(
        runner_name: &str,
        headless: bool,
        http_store: &HttpSessionStore,
        provider: &ProviderConfig,
        registry: &PluginRegistry,
    ) -> Result<Box<dyn Runner + Send>, AilError>;

    /// Build without plugin support (backward-compatible).
    pub fn build(
        runner_name: &str,
        headless: bool,
        http_store: &HttpSessionStore,
        provider: &ProviderConfig,
    ) -> Result<Box<dyn Runner + Send>, AilError>;
}
```

#### Selection Hierarchy

The effective runner for a step is determined in priority order (highest first):

1. **Per-step `runner:` field** in the pipeline YAML — resolved by the executor.
2. **`AIL_DEFAULT_RUNNER` environment variable** — if set and non-empty.
3. **Hardcoded fallback: `"claude"`**.

#### Runner Name Resolution

When a runner name is requested, the factory resolves it in this order:

1. **Built-in runners** — `claude`, `http`/`ollama`, `stub` (case-insensitive, trimmed).
2. **Plugin registry** — discovered plugins from `~/.ail/runners/`.
3. **Error** — `RUNNER_NOT_FOUND` with the list of all known runner names (built-in + plugins).

#### Known Runner Names (Built-in)

| Name | Case-sensitive | Resulting type | Notes |
|---|---|---|---|
| `claude` | No (trimmed, lowercased) | `ClaudeCliRunner` | Production runner; shells out to the `claude` binary |
| `http` | No | `HttpRunner` | Direct OpenAI-compatible API; configurable timeouts, session continuity |
| `ollama` | No | `HttpRunner` | Alias for `http`; identical behaviour |
| `stub` | No | `StubRunner` | Returns a fixed `"stub response"` string; for tests and development |

Plugin runner names are case-sensitive and must be alphanumeric with hyphens/underscores only.

#### Per-Step runner: Field

Any `prompt:` step may declare a `runner:` field to override the default runner for that step only:

```yaml
pipeline:
  - id: review
    prompt: "Review the changes"
    # no runner: — uses the default (AIL_DEFAULT_RUNNER or claude)

  - id: quick-check
    prompt: "Quick check"
    runner: ollama   # uses the HTTP runner for this step

  - id: codex-review
    prompt: "Review with Codex"
    runner: codex    # uses a plugin runner (if installed)
```

Per-step runner overrides inherit the parent session's headless flag (`Session.headless`).

### Plugin Runner System

Users can extend ail with third-party runners without recompiling:

1. **Install the plugin** — place a manifest file and executable in `~/.ail/runners/`.
2. **Use in pipeline YAML** — set `runner: <plugin-name>` on any step.
3. **ail discovers and launches it** — spawns the executable, communicates via JSON-RPC 2.0 over stdin/stdout.

#### Manifest Example

```yaml
# ~/.ail/runners/codex.yaml
name: codex
version: "0.1.0"
executable: codex-ail-runner
protocol_version: "1"
```

#### Protocol Summary

The AIL Runner Plugin Protocol uses JSON-RPC 2.0 over stdin/stdout:

| Method | Direction | Purpose |
|---|---|---|
| `initialize` | ail → plugin | Handshake and capability negotiation |
| `invoke` | ail → plugin | Send prompt, receive response |
| `permission/respond` | ail → plugin | Respond to tool permission requests |
| `shutdown` | ail → plugin | Graceful shutdown |
| `stream/*` | plugin → ail | Streaming notifications (deltas, thinking, tool events, cost) |

Full details: `spec/runner/r10-plugin-protocol.md` and `spec/runner/r11-plugin-discovery.md`.

### Adding a New Runner

**For plugin authors (no recompilation):**
1. Create an executable that speaks the AIL Runner Plugin Protocol.
2. Write a manifest YAML file declaring the runner name and executable path.
3. Place both in `~/.ail/runners/`.

**For Rust contributors (compile-time):**
1. Implement the `Runner` trait in a new module under `ail-core/src/runner/`.
2. Add a match arm in `RunnerFactory::build()` mapping the runner name to the new type.
3. Export the module from `ail-core/src/runner/mod.rs`.

### Further Reading

- `spec/runner/r01-overview.md` — The AIL Runner Contract overview.
- `spec/runner/r02-claude-cli.md` — Claude CLI reference implementation.
- `spec/runner/r05-http-runner.md` — HTTP/Ollama runner specification.
- `spec/runner/r10-plugin-protocol.md` — JSON-RPC plugin protocol specification.
- `spec/runner/r11-plugin-discovery.md` — Plugin manifest format and discovery rules.

---
