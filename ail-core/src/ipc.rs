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
    GenericFilePath, GenericNamespaced, ListenerOptions, ToFsName, ToNsName,
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
