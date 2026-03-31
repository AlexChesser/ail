# Task 03: InvokeOptions and PermissionRequest Cleanup ✓ DONE

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
12. **Doc comment cleanup** (see Phase 6 below)
13. **Spec alignment** (see Phase 6 below)
14. Run full test suite + verify spec grep

## Phase 6: Doc Comment and Spec Alignment

**Motivation:** AIL's long-term goal is runner independence — Claude Code is the first runner, not the only one. After restructuring the abstract types in Phases 1-2, complete the abstraction by removing Claude-specific language from the public-facing surface.

### 6a. Doc comments in `ail-core/src/runner/mod.rs`

Remove all Claude CLI references from doc comments on abstract types. Replace with runner-agnostic descriptions. Target lines (current state — may shift after earlier phases):

- `PermissionRequest` doc: remove "intercepted from Claude CLI via the MCP permission bridge". Replace with: "A tool permission request emitted by the runner when it requires a human decision before executing a tool."
- `PermissionResponder` doc: remove "invoked by the runner when Claude CLI requests tool permission". Replace with: "Callback provided to the runner to resolve tool permission requests. The runner owns its transport (MCP, stdio, HTTP, etc.). The callback blocks until the human decides. Runners that do not support tool permissions ignore this field."
- `InvokeOptions::resume_session_id` doc: remove `"passed as --resume <id>"`. Replace with: "Resumes an existing conversation by session ID. Runners that do not support session continuity ignore this."
- `InvokeOptions::tool_policy` (formerly `allowed_tools`/`denied_tools`): doc should describe the policy concept, not `--allowedTools`/`--disallowedTools` flags.
- `InvokeOptions::permission_responder` doc: remove `ClaudeCliRunner` and Unix socket references. Replace with: "Callback for bidirectional tool permission prompts. When set, the runner should intercept permission requests and call this to obtain a decision before proceeding."
- `invoke_streaming` default impl doc: change "e.g. `ClaudeCliRunner`" to a generic description.

### 6b. Doc comment in `ail-core/src/executor.rs`

`ExecutionControl::permission_responder` doc currently references "MCP bridge (SPEC §13.3)". Change to: "Propagated into `InvokeOptions::permission_responder` for each runner invocation. The runner resolves permission requests via its own mechanism."

### 6c. Spec: `spec/core/s13-hitl-gates.md` — split abstract from concrete

**Current problem:** §13.3 "Tool Permission HITL" describes `ClaudeCliRunner`'s MCP bridge Unix socket protocol as the canonical implementation. A developer writing a non-Claude runner reads this and concludes permission HITL requires MCP + Unix sockets.

**Required change:** Restructure §13.3 into two parts:

**§13.3 (abstract — stays in `spec/core/s13-hitl-gates.md`):**
Replace the current implementation block with an abstract contract:
> When a pipeline step encounters a tool not covered by its `tools.allow`/`tools.deny` policy, the executor passes a `PermissionResponder` callback to the runner via `InvokeOptions`. The runner is responsible for: (1) intercepting the permission decision point in its native protocol, (2) calling the responder with an abstract `PermissionRequest`, and (3) serialising the `PermissionResponse` back to its native protocol. Runners that do not support tool permissions ignore the `permission_responder` field. Runners in headless mode bypass permission HITL entirely.

Then add a cross-reference: "The Claude CLI reference implementation uses an MCP bridge subprocess and Unix domain socket — see `spec/runner/r02-claude-cli.md §Tool Permission Interface`."

Remove the IPC topology diagram and socket lifecycle steps (lines ~30-82 in current file). They are already documented in `spec/runner/r02-claude-cli.md`.

**§13.4 Tool Permission Flow** (the diagram): Replace the Claude-CLI-specific diagram (which references `--allowedTools`, `ail_check_permission`, the MCP bridge) with a runner-agnostic flow:
```
Claude CLI wants to invoke a tool
  ↓
Runner checks its tool policy (from InvokeOptions.tool_policy)
  → Pre-approved? YES → tool executes
  → Pre-denied?   YES → tool denied
  ↓ UNKNOWN
Runner calls PermissionResponder(PermissionRequest{display_name, display_detail})
  ↓
TUI shows permission modal
  → Approve once / Allow for session / Deny
  ↓
PermissionResponse returned to runner → runner native protocol response
```

### 6d. Spec: `spec/runner/r02-claude-cli.md` — no change needed

The Claude-specific socket lifecycle and MCP topology are already correctly placed here. §13.3 in `s13-hitl-gates.md` will cross-reference this file instead of duplicating it.

### 6e. Verification for Phase 6

```bash
# No Claude CLI references in abstract spec sections
grep -n "Claude CLI\|claude CLI\|ClaudeCliRunner\|mcp-bridge\|unix socket\|UnixListener" spec/core/s13-hitl-gates.md
# Should only return the cross-reference line, not implementation details

# No Claude references in abstract type docs
grep -n "Claude\|claude\|--resume\|--allowedTools\|--disallowed\|ANTHROPIC_" ail-core/src/runner/mod.rs
# Should return zero matches
```

## Risks

- **`dyn Any` downcasting** not type-safe at compile time. Mitigated by helper method.
- **`execute_inner` cannot construct Claude extensions** without knowing about Claude. Caller is responsible for pre-populating.
- **PermissionRequest rename** preserves behavior: `display_name` = `tool_name` value.

## Critical Files
- `ail-core/src/runner/mod.rs` — InvokeOptions, PermissionRequest, ToolPermissionPolicy, doc cleanup
- `ail-core/src/runner/claude.rs` — ClaudeInvokeExtensions, spawn_process refactor
- `ail-core/src/executor.rs` — InvokeOptions construction in both execute paths, doc cleanup
- `ail/src/tui/backend.rs` — InvokeOptions construction, PermissionRequest translation
- `ail/src/tui/app.rs` — PermissionRequest field access
- `ail/src/tui/ui/modal.rs` — PermissionRequest field access
- `spec/core/s13-hitl-gates.md` — §13.3 and §13.4 abstract/concrete split
- `ail-core/CLAUDE.md` — update InvokeOptions and PermissionRequest type descriptions to match new types
