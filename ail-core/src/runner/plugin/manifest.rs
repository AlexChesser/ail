//! Validated domain types for runner plugin manifests.
//!
//! These types have no serde derives — they are constructed only through
//! validation in [`super::validation`].

use std::collections::HashMap;
use std::path::PathBuf;

/// A validated runner plugin manifest.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    /// Runner name used in pipeline YAML (`runner: <name>`).
    pub name: String,
    /// Version of the runner extension.
    pub version: String,
    /// Resolved absolute path to the runner executable.
    pub executable: PathBuf,
    /// Protocol version the runner speaks.
    pub protocol_version: String,
    /// Environment variables to pass to the runner subprocess.
    pub env: HashMap<String, String>,
    /// Command-line arguments to pass to the runner executable.
    pub args: Vec<String>,
    /// Path to the manifest file itself (for diagnostics).
    pub manifest_path: PathBuf,
}
