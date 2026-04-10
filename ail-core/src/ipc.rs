//! Cross-platform local IPC helpers for the permission bridge.
//!
//! On Unix (Linux, macOS) this wraps `interprocess` filesystem-based local
//! sockets (Unix domain sockets). On Windows it wraps named pipes via the same
//! `interprocess` crate. Both transports implement `Read + Write`; the
//! higher-level JSON-line protocol in the permission bridge is identical on all
//! platforms.
//!
//! The public surface is intentionally minimal — just enough to replace the
//! `std::os::unix::net` calls in `runner/claude.rs` and `mcp_bridge.rs`.

use std::io;

#[allow(unused_imports)] // GenericNamespaced/ToNsName used only on non-Unix
use interprocess::local_socket::{
    prelude::*, // brings LocalSocketListener, LocalSocketStream, Stream, ListenerExt into scope
    GenericFilePath,
    GenericNamespaced,
    ListenerOptions,
    ToFsName,
    ToNsName,
};

// Re-export so callers in this crate can annotate types without depending on
// interprocess directly.
pub use interprocess::local_socket::prelude::LocalSocketStream as IpcStream;

/// Generate a unique address string for a new permission socket.
///
/// On Unix the address is an absolute path to a temporary file
/// (`$TMPDIR/ail-perm-<uuid>.sock`). On Windows it is a pipe name
/// (`ail-perm-<uuid>`), which `interprocess` maps to `\\.\pipe\ail-perm-<uuid>`.
pub fn generate_address() -> String {
    let id = uuid::Uuid::new_v4();
    #[cfg(unix)]
    {
        std::env::temp_dir()
            .join(format!("ail-perm-{id}.sock"))
            .to_string_lossy()
            .into_owned()
    }
    #[cfg(not(unix))]
    {
        format!("ail-perm-{id}")
    }
}

/// Bind a new local socket listener at the given address.
///
/// Pass an address produced by [`generate_address`]. On Unix the address is
/// treated as a filesystem path; on Windows as a named-pipe name.
pub fn bind_local(address: &str) -> io::Result<LocalSocketListener> {
    #[cfg(unix)]
    let name = address.to_fs_name::<GenericFilePath>()?;
    #[cfg(not(unix))]
    let name = address.to_ns_name::<GenericNamespaced>()?;
    ListenerOptions::new().name(name).create_sync()
}

/// Connect to a local socket at the given address.
///
/// The address must be one that was previously passed to [`bind_local`] on the
/// same machine.
pub fn connect_local(address: &str) -> io::Result<LocalSocketStream> {
    #[cfg(unix)]
    let name = address.to_fs_name::<GenericFilePath>()?;
    #[cfg(not(unix))]
    let name = address.to_ns_name::<GenericNamespaced>()?;
    LocalSocketStream::connect(name)
}

/// Clean up the socket address after use.
///
/// On Unix this deletes the socket file from the filesystem. On Windows named
/// pipes are kernel objects and require no explicit cleanup.
pub fn cleanup_address(address: &str) {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(address);
    }
    #[cfg(not(unix))]
    {
        let _ = address; // no-op on Windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_address_returns_non_empty_string() {
        let addr = generate_address();
        assert!(
            !addr.is_empty(),
            "generate_address() must return a non-empty string"
        );
    }

    #[test]
    fn generate_address_contains_ail_perm_prefix() {
        let addr = generate_address();
        assert!(
            addr.contains("ail-perm-"),
            "address should contain the 'ail-perm-' prefix, got: {addr}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn generate_address_on_unix_has_sock_suffix() {
        let addr = generate_address();
        assert!(
            addr.ends_with(".sock"),
            "Unix address should end with '.sock', got: {addr}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn generate_address_on_unix_is_absolute_path() {
        let addr = generate_address();
        assert!(
            addr.starts_with('/'),
            "Unix address should be an absolute path, got: {addr}"
        );
    }

    #[test]
    fn generate_address_two_calls_return_different_addresses() {
        let addr1 = generate_address();
        let addr2 = generate_address();
        assert_ne!(
            addr1, addr2,
            "each call to generate_address() should return a unique address"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_address_on_nonexistent_file_is_silent() {
        // Cleaning up a path that does not exist should not panic or return an error.
        cleanup_address("/tmp/ail-perm-does-not-exist-at-all.sock");
        // If we reach here without panic, the test passes.
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_address_removes_existing_file() {
        let addr = generate_address();
        // Create the file so it exists.
        std::fs::write(&addr, b"").expect("write temp socket file");
        assert!(
            std::path::Path::new(&addr).exists(),
            "file should exist before cleanup"
        );
        cleanup_address(&addr);
        assert!(
            !std::path::Path::new(&addr).exists(),
            "file should be removed after cleanup"
        );
    }
}
