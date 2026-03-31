# Task 02: Permission Bridge Extraction

## Prerequisites
- Task 01 (runner injection through TUI) — establishes runner as injected dependency

## Findings Addressed
- **DIP-002** (high): Claude MCP permission wire protocol encoded in TUI backend (lines 85-161)
- **RUNNER-002** (high): Permission bridge has no home in the Runner trait
- **SRP-002** (medium, permission portion): spawn_backend owns permission socket lifecycle

## Problem Summary

The Unix socket permission bridge protocol (bind socket, accept connections, read JSON `{tool_name, tool_input}`, serialize `{behavior, updatedInput/message}`) is Claude MCP-specific but lives in `ail/src/tui/backend.rs`. The TUI should not know about Unix sockets or MCP JSON wire formats. It should only deal with abstract `PermissionRequest` and `PermissionResponse` types.

## Design: Callback-based PermissionBridge

Rather than a separate trait, `ClaudeCliRunner` will own the entire socket lifecycle internally. A `PermissionResponder` callback type allows the runner to ask for permission decisions without the caller knowing the transport.

### Step 1: Add `PermissionResponder` type to `InvokeOptions`

**File:** `ail-core/src/runner/mod.rs`

```rust
pub type PermissionResponder = Arc<dyn Fn(PermissionRequest) -> PermissionResponse + Send + Sync>;
```

Add to `InvokeOptions`:
```rust
pub permission_responder: Option<PermissionResponder>,
```

Remove `permission_socket: Option<PathBuf>` from `InvokeOptions`.

### Step 2: Move socket server logic into `ClaudeCliRunner`

**File:** `ail-core/src/runner/claude.rs`

Add a private function:
```rust
fn spawn_permission_listener(
    responder: PermissionResponder,
    event_tx: mpsc::Sender<RunnerEvent>,
) -> Result<(PathBuf, JoinHandle<()>, mpsc::Receiver<()>), AilError>
```

This encapsulates:
1. Generate temp socket path (`ail-perm-<uuid>.sock`)
2. Bind `UnixListener`, signal readiness
3. Accept loop: read JSON → parse to `PermissionRequest` → emit `RunnerEvent::PermissionRequested` → call `responder(req)` → serialize response → write back
4. Return socket path for `spawn_process`

### Step 3: Integrate into `invoke_streaming`

**File:** `ail-core/src/runner/claude.rs`

In `invoke_streaming`:
1. If `options.permission_responder.is_some()` and `!self.headless`, call `spawn_permission_listener`
2. Wait for ready signal
3. Pass socket path to `spawn_process` as separate parameter (not via InvokeOptions)
4. After CLI exits, clean up socket file

Refactor `spawn_process` to accept `permission_socket: Option<&Path>` as a parameter instead of reading from `options`.

### Step 4: Remove `permission_socket` from `ExecutionControl`

**File:** `ail-core/src/executor.rs`

Replace `permission_socket: Option<PathBuf>` on `ExecutionControl` with a responder. Two options:

- **Option B (pragmatic):** Store the responder in `ExecutionControl` replacing `permission_socket`. Simpler, keeps diff smaller.

Update `execute_with_control` to pass `permission_responder` into each `InvokeOptions`.

### Step 5: Simplify `backend.rs`

**File:** `ail/src/tui/backend.rs`

Remove:
- Lines 85-166: entire socket creation, listener thread, ready-wait block
- `BackendEvent::PermReady` variant (optional — see below)
- `std::os::unix::net::UnixListener` import

Replace with a `PermissionResponder` closure:
```rust
let (perm_tx, perm_rx) = mpsc::channel::<PermissionResponse>();
let _ = event_tx.send(BackendEvent::PermReady(perm_tx));

let perm_event_tx = event_tx.clone();
let perm_rx = Arc::new(Mutex::new(perm_rx));
let responder: PermissionResponder = Arc::new(move |req: PermissionRequest| {
    let _ = perm_event_tx.send(BackendEvent::PermissionRequest(req));
    let rx = perm_rx.lock().unwrap();
    rx.recv().unwrap_or(PermissionResponse::Deny("channel closed".into()))
});
```

The TUI-facing API (`BackendEvent::PermissionRequest`, `BackendEvent::PermReady`, `PermissionResponse`) is unchanged. AppState and inline/mod.rs need no changes.

### Thread Safety Note

`mpsc::Receiver` is `Send` but not `Sync`. Wrapping in `Mutex` makes it `Sync`, enabling use inside `Arc<dyn Fn + Send + Sync>`. This works because permission requests are serialized (one at a time).

### Cleanup Semantics

Each `invoke_streaming` call that uses a permission socket creates and cleans up its own socket (per-call). This is correct since each step is a separate CLI invocation.

## Testing

- Unit test: `spawn_permission_listener` in isolation — connect to socket, send mock JSON, verify responder called
- Existing TUI permission flow unchanged (same BackendEvent types)
- Verify headless mode: no socket created

## Critical Files
- `ail-core/src/runner/mod.rs` — add `PermissionResponder`, remove `permission_socket` from `InvokeOptions`
- `ail-core/src/runner/claude.rs` — move socket listener, integrate into `invoke_streaming`
- `ail/src/tui/backend.rs` — remove socket code, create responder closure
- `ail-core/src/executor.rs` — remove `permission_socket` from `ExecutionControl`, thread responder
