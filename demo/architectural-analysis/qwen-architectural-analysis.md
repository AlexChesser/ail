# AIL TUI & Core Architecture — Findings

**Analyst:** Claude Sonnet 4.6 | **Date:** 2026-03-28 | **Scope:** `ail/src/tui/`, `ail/src/main.rs`, `ail/src/cli.rs`, `ail-core/src/executor.rs`, `ail-core/src/runner/`, `ail-core/src/session/`

## Findings

| ID | Principle | Category | Severity | Component | Location | Summary |
|----|-----------|----------|----------|-----------|----------|---------|
| SRP-001 | Single Responsibility | God Object | **high** | `tui/app.rs :: AppState` | `ail/src/tui/app.rs` (~873 lines) | AppState has at least 7 distinct responsibilities: pipeline metadata, execution phase state machine, picker/hot-reload UI, interrupt/pause/kill modal, multi-line prompt input/history, viewport scrollback, run statistics, and tool permission HITL modal |
| SRP-002 | Single Responsibility | Mixed Concerns | **medium** | `tui/backend.rs :: spawn_backend()` | `ail/src/tui/backend.rs:1-269` | spawn_backend() owns thread spawning, runner construction, session setup, permission socket lifecycle, event bridging, and HITL coordination |
| SRP-003 | Single Responsibility | Mixed Concerns | **low** | `tui/inline/mod.rs` | `ail/src/tui/inline/mod.rs` | Inline event loop mixes terminal lifecycle, event polling, scrollback flushing, drawing coordination, and backend submission |
| OCP-001 | Open/Closed | Closed to Extension | **medium** | `BackendEvent` enum | `ail/src/tui/backend.rs:27-42` | Adding a new backend event type requires editing the enum and every match arm that consumes it |
| OCP-002 | Open/Closed | Closed to Extension | **low** | `ExecutorEvent` enum | `ail-core/src/executor.rs` | ExecutorEvent is closed; adding step types or execution events requires modifying all consumers |
| DIP-001 | Dependency Inversion | Hardcoded Concrete | **critical** | `tui/backend.rs :: ClaudeCliRunner` | `ail/src/tui/backend.rs:12,56` | backend.rs directly imports and constructs ClaudeCliRunner — the TUI is hardwired to Claude |
| DIP-002 | Dependency Inversion | Leaking Concrete | **high** | `tui/backend.rs :: Permission Socket Protocol` | `ail/src/tui/backend.rs:85-161` | The Claude MCP permission wire protocol (Unix socket + JSON fields) is encoded directly in the TUI backend, not in the runner |
| DIP-003 | Dependency Inversion | Leaking Concrete | **medium** | `tui/app.rs :: PermissionRequest fields` | `ail/src/tui/app.rs` | AppState and modal UI directly reference `PermissionRequest.tool_name` and `PermissionRequest.tool_input` — runner-specific concepts in the UI layer |
| ISP-001 | Interface Segregation | Fat Interface | **low** | `Runner` trait | `ail-core/src/runner/mod.rs` | Runner trait bundles both non-streaming and streaming invocation; StubRunner must implement both even if only one is tested |
| LSP-001 | Liskov Substitution | Concrete Leak | **medium** | `ClaudeCliRunner::new(headless: bool)` | `ail-core/src/runner/claude.rs` | The headless flag is a Claude CLI concept (`--dangerously-skip-permissions`) leaking into the construction contract |
| COUPLING-001 | Loose Coupling | Tight Coupling — Runner Selection | **critical** | Full TUI stack | `ail/src/tui/backend.rs:12,56` and `ail/src/main.rs` | The entire TUI is coupled to Claude: runner selection is not a plug point anywhere in the TUI call stack |
| COUPLING-002 | Loose Coupling | Tight Coupling — Session Resumption | **medium** | `TurnEntry.runner_session_id` | `ail-core/src/session/` | session_id / runner_session_id is a Claude CLI concept — session resumption is assumed to be universally available |
| COUPLING-003 | Loose Coupling | Tight Coupling — Tool Allowlist Format | **medium** | `InvokeOptions.allowed_tools / disallowed_tools` | `ail-core/src/runner/mod.rs` | Tool allowlists/denylists use Claude CLI FQN format (`mcp__server__tool`) embedded in generic InvokeOptions |
| COUPLING-004 | Loose Coupling | Tight Coupling — MCP Bridge | **high** | `runner/claude.rs :: mcp_config handling` | `ail-core/src/runner/claude.rs` | MCP server config and `mcp_bridge` FQN tool references are Claude-specific capabilities wired into InvokeOptions |
| DI-001 | Dependency Injection | Missing Injection Point | **high** | `tui/backend.rs :: spawn_backend signature` | `ail/src/tui/backend.rs:47-51` | spawn_backend() accepts pipeline and cli_provider but not the runner — the most variable dependency is hardcoded |
| DI-002 | Dependency Injection | Missing Injection Point | **low** | Session construction in backend.rs | `ail/src/tui/backend.rs:68-69` | Session is constructed inside the backend thread with no factory abstraction |
| CYCLIC-001 | Acyclic Dependencies | No Cycles Detected | **none** | `ail` / `ail-core` crate boundary | `Cargo.toml` | The crate-level dependency graph is acyclic and compiler-enforced |
| CYCLIC-002 | Acyclic Dependencies | Logical Cycle Risk | **low** | `app.rs` ↔ `backend.rs` | `ail/src/tui/` | app.rs and backend.rs share types (BackendCommand, BackendEvent, PermissionRequest) creating logical bi-directional coupling within the same crate |
| RUNNER-001 | Runner Interface Integrity | Runner Abstraction Leakage | **critical** | Runner trait vs. Claude-specific surface area | `ail-core/src/runner/mod.rs`, `ail/src/tui/backend.rs` | The Runner trait is sound but the surrounding infrastructure (InvokeOptions, permission socket, session ID) carries so much Claude-specific surface area that substituting a different runner is practically impossible without code changes |
| RUNNER-002 | Runner Interface Integrity | Interface Gap | **high** | Permission bridge ownership | `ail/src/tui/backend.rs:85-161` | The permission bridge (Unix socket listener + MCP JSON protocol) has no home in the Runner trait — it is a runner-level concern stranded in the TUI backend |
| QUALITY-001 | Code Quality | Testability | **medium** | `tui/` — untestable state machine | `ail/src/tui/app.rs`, `ail/src/tui/inline/mod.rs` | AppState and the inline event loop have no unit tests and are structurally resistant to testing |
| QUALITY-002 | Code Quality | Error Handling | **low** | backend.rs permission socket listener | `ail/src/tui/backend.rs:141-143` | `perm_rx.recv()` fallback to Deny silently swallows channel-closed errors |
| QUALITY-003 | Code Quality | Inline Magic | **low** | `tui/inline/layout.rs` | `ail/src/tui/inline/layout.rs` | Viewport height (10 rows, 9 chrome rows, 1 status) are magic numbers likely duplicated across inline/mod.rs and draw.rs |

## Summary

**Total findings:** 21

### By Severity

| Severity | Count |
|----------|-------|
| Critical | 3 |
| High | 5 |
| Medium | 7 |
| Low | 5 |
| None | 1 |

### By Principle

| Principle | Count |
|-----------|-------|
| Single Responsibility | 3 |
| Open/Closed | 2 |
| Liskov Substitution | 1 |
| Interface Segregation | 1 |
| Dependency Inversion | 3 |
| Dependency Injection | 2 |
| Loose Coupling | 4 |
| Acyclic Dependencies | 2 |
| Runner Interface Integrity | 2 |
| Code Quality | 3 |

### Top Risks

1. **DIP-001 / COUPLING-001 / DI-001:** ClaudeCliRunner is hardcoded in spawn_backend — there is no injection seam at the TUI level. The Runner trait abstraction that exists in ail-core is completely bypassed by the TUI.
2. **DIP-002 / RUNNER-002:** The Claude MCP permission wire protocol lives in the TUI backend, not in the runner. This is a runner-level concern stranded in the wrong layer.
3. **RUNNER-001:** The surrounding infrastructure (InvokeOptions, permission socket, session ID) carries so much Claude-specific surface area that swapping the runner requires changes to backend.rs, app.rs, and InvokeOptions — not just the runner implementation.
4. **SRP-001:** AppState is an 873-line god object. Every new TUI feature accretes here, making the module a perpetual merge conflict magnet.

### What Is Working Well

- The `ail` / `ail-core` crate boundary is hard and compiler-enforced — the strongest guarantee in the system.
- `executor.rs` takes `&dyn Runner` — the core execution path is correctly dependency-inverted.
- The DTO→Domain boundary in `config/` is clean — serde never leaks into domain types.
- Event streaming via channels (`RunnerEvent` → `ExecutorEvent` → `BackendEvent`) is a sound decoupling pattern.
- Atomic pause/kill flags are a clean, non-blocking control mechanism.
- The Runner trait itself (`invoke` + `invoke_streaming`) is well-scoped and implementable.
- `StubRunner` exists and provides the right test seam at the executor level.
