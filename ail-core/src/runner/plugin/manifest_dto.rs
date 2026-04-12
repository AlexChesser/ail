//! Serde DTOs for runner plugin manifest files.
//!
//! These are the raw deserialised structs from `~/.ail/runners/<name>.yaml`.
//! They are converted to domain types in [`super::validation`].

use serde::Deserialize;
use std::collections::HashMap;

/// Raw manifest as read from YAML/JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestDto {
    /// Runner name used in pipeline YAML (`runner: <name>`).
    pub name: Option<String>,
    /// Version of the runner extension.
    pub version: Option<String>,
    /// Path to the runner executable.
    /// Can be: absolute, relative to manifest, or a bare name (looked up on PATH).
    pub executable: Option<String>,
    /// Protocol version the runner speaks (currently only "1").
    pub protocol_version: Option<String>,
    /// Optional environment variables to pass to the runner subprocess.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Optional command-line arguments to pass to the runner executable.
    #[serde(default)]
    pub args: Vec<String>,
}
