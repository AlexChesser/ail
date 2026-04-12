//! Runner plugin system — runtime-discoverable runner extensions.
//!
//! A runner plugin is an executable that speaks the AIL Runner Plugin Protocol
//! (JSON-RPC 2.0 over stdin/stdout). Users install plugins by placing a manifest
//! file in `~/.ail/runners/` alongside the plugin binary.
//!
//! See `spec/runner/r10-plugin-protocol.md` for the full protocol specification
//! and `spec/runner/r11-plugin-discovery.md` for manifest and discovery rules.

#![allow(clippy::result_large_err)]

pub mod discovery;
pub mod jsonrpc;
pub mod manifest;
pub mod manifest_dto;
pub mod protocol_runner;
pub mod validation;

pub use discovery::{discover_plugins, discover_plugins_in, PluginRegistry};
pub use manifest::PluginManifest;
pub use protocol_runner::ProtocolRunner;

use crate::runner::subprocess::SubprocessSpec;

/// Build a [`SubprocessSpec`] from a plugin manifest.
///
/// This translates the manifest's executable path, args, and env into the
/// format expected by [`SubprocessSession::spawn`].
fn subprocess_spec_from_manifest(manifest: &PluginManifest) -> SubprocessSpec {
    SubprocessSpec {
        program: manifest.executable.to_string_lossy().to_string(),
        args: manifest.args.clone(),
        env_remove: vec![],
        env_set: manifest
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}
