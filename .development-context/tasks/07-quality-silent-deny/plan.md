# Task 07: Fix Silent Deny on Channel Close ✓ DONE

## Findings Addressed
- **QUALITY-002** (low): perm_rx.recv() fallback to Deny silently swallows channel-closed errors

## Can Be Done Independently

No dependencies. Small, isolated fix.

## Problem Summary

In `ail/src/tui/backend.rs` lines 141-143, when the permission response channel closes unexpectedly (e.g., TUI panics), tool calls are silently denied. This violates ARCHITECTURE.md §2.8: "No silent failures."

Current code:
```rust
let response = perm_rx
    .recv()
    .unwrap_or(PermissionResponse::Deny("channel closed".into()));
```

## Implementation

**File:** `ail/src/tui/backend.rs`

Replace with:
```rust
let response = match perm_rx.recv() {
    Ok(r) => r,
    Err(_) => {
        tracing::error!(
            "permission: response channel closed unexpectedly; aborting run"
        );
        let _ = etx.send(BackendEvent::Error(
            "Permission response channel closed unexpectedly. \
             The current run has been aborted."
                .to_string(),
        ));
        // Send a deny to the current connection so Claude CLI gets a clean response
        let deny_json = serde_json::json!({
            "behavior": "deny",
            "message": "Permission channel closed; run aborted"
        });
        let mut deny_line = serde_json::to_string(&deny_json).unwrap_or_default();
        deny_line.push('\n');
        let _ = conn.write_all(deny_line.as_bytes());
        break; // Exit the listener loop
    }
};
```

This:
1. Logs at ERROR level via tracing (satisfies "no silent failures")
2. Sends `BackendEvent::Error` to TUI (causes `ExecutionPhase::Failed`)
3. Sends a clean deny to the current socket connection
4. Breaks the listener loop (no further connections accepted)

## Why Not Also Flip `kill_requested`?

The listener thread doesn't have access to the `kill_requested` Arc. Breaking the loop closes the socket, so the Claude CLI subprocess will fail on its next permission request (connection refused). Sufficient for now.

## Testing

- Manual: start a run with permission-gated tools, kill TUI, verify error logged and run marked failed
- `BackendEvent::Error` is already handled by inline/mod.rs (sets phase to Failed)

## Risk Assessment

Very low. 10-15 lines in a single file. Uses existing `BackendEvent::Error` infrastructure.

## Critical Files
- `ail/src/tui/backend.rs` — lines 141-143 in permission listener closure (only file modified)
