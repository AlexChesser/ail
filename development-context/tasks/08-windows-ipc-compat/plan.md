# Task 08: Cross-Platform IPC for Permission Bridge

## Status: IMPLEMENTED

## Problem

`cargo build` fails on Windows because `std::os::unix::net::{UnixListener, UnixStream}`
does not exist on that platform. Two files were affected:

| File | Line | Usage |
|---|---|---|
| `ail-core/src/runner/claude.rs` | 4, 206 | `UnixListener::bind()` — server |
| `ail/src/mcp_bridge.rs` | 13, 138 | `UnixStream::connect()` — client |

## Solution

Added `interprocess = "2.4"` (resolved from `"2.2"` spec) and introduced
`ail-core/src/ipc.rs` — a thin cross-platform wrapper around the crate's
`LocalSocketListener` / `LocalSocketStream` types.

| Platform | Transport | Name format |
|---|---|---|
| Unix (Linux, macOS) | Unix domain socket | `/tmp/ail-perm-<uuid>.sock` (filesystem path) |
| Windows | Named pipe | `ail-perm-<uuid>` → `\\.\pipe\ail-perm-<uuid>` |

Both transports implement `Read + Write` identically; the JSON-line wire protocol
between the permission bridge server and client is unchanged.

## Files Changed

| File | Change |
|---|---|
| `ail-core/Cargo.toml` | Added `interprocess = "2.2"` |
| `ail/Cargo.toml` | Added `interprocess = "2.2"` |
| `ail-core/src/lib.rs` | Added `pub mod ipc;` |
| `ail-core/src/ipc.rs` | **New file** — `generate_address()`, `bind_local()`, `connect_local()`, `cleanup_address()`, `IpcStream` re-export |
| `ail-core/src/runner/claude.rs` | Removed `UnixListener`; use `crate::ipc::bind_local()`; `permission_socket: Option<PathBuf>` → `Option<String>`; `write_mcp_config` parameter `&Path` → `&str`; socket cleanup via `crate::ipc::cleanup_address()` |
| `ail/src/mcp_bridge.rs` | Removed `UnixStream`; use `ail_core::ipc::connect_local()` |

## Design Notes

- `generate_address()` returns an opaque string — filesystem path on Unix, pipe
  name on Windows. Neither server nor client needs to know which.
- Socket cleanup (deleting the socket file) is handled by `cleanup_address()`,
  which is a no-op on Windows.
- `ail serve` (planned v0.2) is unaffected — its HITL path uses HTTP endpoints,
  not the permission bridge IPC channel.
- The `interprocess` dependency resolves to v2.4.0. The sync (non-tokio) API is
  used; no feature flags needed.

## Verification

```bash
cargo build         # clean on Linux
cargo clippy -- -D warnings  # clean on Linux
```

Windows: `cargo build` should now succeed. The permission bridge flow
(`ClaudeCliRunner` + `mcp-bridge` subprocess) works identically on all platforms.
