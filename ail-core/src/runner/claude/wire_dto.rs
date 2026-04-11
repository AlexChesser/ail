//! Serde DTOs for the Claude CLI `--output-format stream-json` wire format.
//!
//! These types are used internally by [`super::decoder`] to deserialize content blocks
//! in a single typed pass rather than using stringly-typed `serde_json::Value` indexing.

use serde::Deserialize;

/// A single content block from an `assistant` or `user` message.
///
/// Fields are optional because different block types carry different fields.
/// The `block_type` field determines which other fields are populated.
#[derive(Debug, Deserialize)]
pub(super) struct WireContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    // "text" blocks
    pub text: Option<String>,
    // "thinking" blocks
    pub thinking: Option<String>,
    // "tool_use" blocks
    pub name: Option<String>,
    pub id: Option<String>,
    pub input: Option<serde_json::Value>,
    // "tool_result" blocks
    pub tool_use_id: Option<String>,
    pub content: Option<serde_json::Value>,
    pub is_error: Option<bool>,
    pub tool_name: Option<String>,
}

/// Token usage from an `assistant` message.
#[derive(Debug, Deserialize, Default)]
pub(super) struct WireUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}
