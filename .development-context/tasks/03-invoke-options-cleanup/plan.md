# Task 03: InvokeOptions and PermissionRequest Cleanup

## Prerequisites
- Task 01 (runner injection) and Task 02 (permission bridge extraction)

## Findings Addressed
- **COUPLING-003** (medium): allowed_tools/denied_tools use Claude CLI FQN format
- **COUPLING-004** (high): mcp_config and mcp_bridge_tool_fqn are Claude-only fields
- **DIP-003** (medium): PermissionRequest.tool_name and .tool_input are Claude MCP concepts

## Problem Summary

`InvokeOptions` and `PermissionRequest` in `ail-core/src/runner/mod.rs` carry Claude-specific concepts that would not apply to other runners, violating ARCHITECTURE.md §2.6 (LSP).

## Design

### Phase 1: Restructure InvokeOptions

**New types in `ail-core/src/runner/mod.rs`:**

```rust
#[derive(Debug, Clone, Default)]
pub enum ToolPermissionPolicy {
    #[default]
    RunnerDefault,
    Allowlist(Vec<String>),
    Denylist(Vec<String>),
    Mixed { allow: Vec<String>, deny: Vec<String> },
}

#[derive(Default)]
pub struct InvokeOptions {
    pub resume_session_id: Option<String>,
    pub tool_policy: ToolPermissionPolicy,
    pub model: Option<String>,
    pub extensions: Option<Box<dyn std::any::Any + Send>>,
}
```

**New type in `ail-core/src/runner/claude.rs`:**

```rust
#[derive(Debug, Clone, Default)]
pub struct ClaudeInvokeExtensions {
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
    pub permission_socket: Option<PathBuf>,
}
```

### Phase 2: Restructure PermissionRequest

```rust
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub display_name: String,
    pub display_detail: String,
}
```

Each runner maps its native format to this. The Claude-specific `tool_input` JSON formatting (currently in `app.rs` lines 443-466) moves to `backend.rs` where the request is constructed from Claude's JSON.

### Phase 3: Update ClaudeCliRunner::spawn_process

Extract `ClaudeInvokeExtensions` from `options.extensions` via `downcast_ref`. Map `ToolPermissionPolicy` to CLI args:
- `RunnerDefault` → no flags
- `Allowlist(v)` → `--allowedTools v.join(",")`
- `Denylist(v)` → `--disallowedTools v.join(",")`
- `Mixed{a,d}` → both flags

Add helper: `ClaudeInvokeExtensions::from_options(opts: &InvokeOptions) -> Option<&Self>`

### Phase 4: Update all InvokeOptions construction sites

4 sites total:
1. `executor.rs` `execute_inner` (line 226-242)
2. `executor.rs` `execute_with_control` (line 505-521)
3. `backend.rs` invocation step (line 186-192)
4. `main.rs` --once mode (line 124-129)

**Pragmatic decision:** Keep `permission_socket` on `ExecutionControl` for now. The executor packs it into `ClaudeInvokeExtensions` when building `InvokeOptions`. Document as "to be cleaned up in task 04 when runner config is fully injected."

### Phase 5: Update PermissionRequest consumers

1. `app.rs` `handle_permission_request` — change `req.tool_name` to `req.display_name`, `req.tool_input` to `req.display_detail`
2. `app.rs` `perm_session_allowlist` — key on `display_name`
3. `ui/modal.rs` — use `display_name`
4. `backend.rs` — translate Claude JSON to generic `{ display_name, display_detail }` at construction

## Implementation Sequence

1. Define `ToolPermissionPolicy` enum
2. Restructure `InvokeOptions`
3. Define `ClaudeInvokeExtensions`
4. Update `ClaudeCliRunner::spawn_process`
5. Add `provider_extensions` threading through executor
6. Update executor InvokeOptions construction
7. Update backend.rs and main.rs construction
8. Restructure `PermissionRequest`
9. Move Claude formatting logic from app.rs to backend.rs
10. Update app.rs, modal.rs field access
11. Update StubRunner tests
12. Run full test suite

## Risks

- **`dyn Any` downcasting** not type-safe at compile time. Mitigated by helper method.
- **`execute_inner` cannot construct Claude extensions** without knowing about Claude. Caller is responsible for pre-populating.
- **PermissionRequest rename** preserves behavior: `display_name` = `tool_name` value.

## Critical Files
- `ail-core/src/runner/mod.rs` — InvokeOptions, PermissionRequest, ToolPermissionPolicy
- `ail-core/src/runner/claude.rs` — ClaudeInvokeExtensions, spawn_process refactor
- `ail-core/src/executor.rs` — InvokeOptions construction in both execute paths
- `ail/src/tui/backend.rs` — InvokeOptions construction, PermissionRequest translation
- `ail/src/tui/app.rs` — PermissionRequest field access
- `ail/src/tui/ui/modal.rs` — PermissionRequest field access
