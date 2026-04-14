//! Serde DTOs for the `codex exec --json` wire format.
//!
//! These types are used internally by [`super::decoder`] to deserialize NDJSON events
//! in a single typed pass rather than using stringly-typed `serde_json::Value` indexing.
//!
//! The Codex CLI uses an item lifecycle model: each logical work item (agent message,
//! command execution, reasoning) progresses through `item.started` → `item.updated` →
//! `item.completed` events. The session is identified by `thread_id` from `thread.started`.

use serde::Deserialize;

/// A top-level NDJSON event line from `codex exec --json`.
///
/// Fields are `Option<T>` because different event types carry different subsets of fields.
/// The `event_type` field determines which others are populated.
#[derive(Debug, Deserialize)]
pub(super) struct WireCodexEvent {
    /// Discriminant field — one of: `thread.started`, `item.started`, `item.updated`,
    /// `item.completed`, `turn.completed`, `turn.failed`, `error`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Present on `thread.started`. Becomes `session_id` in [`RunResult`].
    pub thread_id: Option<String>,
    /// Present on all `item.*` events. Used as `tool_use_id` for command execution items.
    pub item_id: Option<String>,
    /// Item category — `"agent_message"`, `"command_execution"`, `"reasoning"`, etc.
    /// Present on all `item.*` events.
    pub item_type: Option<String>,
    /// Item payload. Present on `item.started`, `item.updated`, `item.completed`.
    pub item: Option<WireItem>,
    /// Human-readable error. Present on `turn.failed`.
    pub error: Option<String>,
    /// Human-readable message. Present on the `error` event type.
    pub message: Option<String>,
}

/// Contents of an item, as carried in `item.started` / `item.updated` / `item.completed`.
///
/// Different item types populate different fields; all are optional.
#[derive(Debug, Deserialize)]
pub(super) struct WireItem {
    /// Narrative text. Present on `agent_message` and `reasoning` items.
    pub text: Option<String>,
    /// Shell command. Present on `command_execution` items at `item.started`.
    pub command: Option<String>,
    /// Combined stdout+stderr. Present on `command_execution` items at `item.completed`.
    pub aggregated_output: Option<String>,
    /// Shell exit code. Present on `command_execution` items at `item.completed`.
    pub exit_code: Option<i32>,
}
