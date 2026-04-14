//! Stateful decoder for the `codex exec --json` NDJSON wire format.
//!
//! [`CodexNdjsonDecoder`] accepts NDJSON lines from the Codex CLI one at a time via
//! [`CodexNdjsonDecoder::feed`]. It emits [`RunnerEvent`]s for streaming consumers and
//! accumulates state needed to build the final [`RunResult`] when the terminal
//! `turn.completed` event arrives. Decoding is completely decoupled from process
//! lifecycle — these functions can be tested with raw byte strings, with no subprocess
//! involved.

#![allow(clippy::result_large_err)]

use std::sync::mpsc;

use super::super::{RunResult, RunnerEvent, ToolEvent};
use super::wire_dto::WireCodexEvent;
use crate::error::AilError;

// ── Internal parse action ─────────────────────────────────────────────────────────────────────

/// Terminal outcome of processing a single NDJSON event.
enum ParseAction {
    /// Non-terminal event — any `RunnerEvent`s were already sent through `tx`.
    Continue,
    /// `turn.completed` — the stream finished successfully.
    TurnCompleted,
    /// `turn.failed` or `error` event arrived.
    TurnError(String),
}

// ── Decoder ───────────────────────────────────────────────────────────────────────────────────

/// Accumulates state from a stream of `codex exec --json` NDJSON events.
///
/// Call [`feed`](CodexNdjsonDecoder::feed) for each non-empty line from the Codex CLI
/// stdout. When [`is_done`](CodexNdjsonDecoder::is_done) returns `true`, call
/// [`finalize`](CodexNdjsonDecoder::finalize) to consume the decoder into a [`RunResult`].
///
/// If the stream ended with a `turn.failed` or `error` event, [`finalize`] returns
/// [`AilError::RunnerInvocationFailed`]. If the stream ended without `turn.completed`,
/// [`finalize`] also returns an error.
///
/// Token counts are not available in the `codex exec --json` wire format; `RunResult`
/// fields `input_tokens` and `output_tokens` will always be `0`.
#[derive(Default)]
pub struct CodexNdjsonDecoder {
    /// Final response text. Overwritten each time an `item.completed` / `agent_message`
    /// event arrives (the last one wins, which is the correct final answer).
    response: String,
    /// `thread_id` from `thread.started` — becomes `session_id` in the `RunResult`.
    thread_id: Option<String>,
    /// Concatenated reasoning text accumulated from `reasoning` items.
    thinking: String,
    /// Ordered tool call and tool result events captured during this invocation.
    tool_events: Vec<ToolEvent>,
    /// Monotonically increasing sequence number within this invocation.
    tool_seq: i64,
    /// Set by a `turn.failed` / `error` event or a JSON parse failure.
    error: Option<String>,
    /// True when `turn.completed` (or a terminal error event) has been processed.
    done: bool,
}

impl CodexNdjsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one NDJSON line from the Codex CLI stdout.
    ///
    /// Emits zero or more [`RunnerEvent`]s on `tx` (when `Some`) for streaming consumers.
    /// Sets [`is_done`](Self::is_done) to `true` when a terminal event is seen.
    ///
    /// On a JSON parse error the internal error is set, `is_done()` becomes `true`, and
    /// no further calls to `feed` will have any effect.
    ///
    /// Returns `Err(detail)` only for JSON parse failures so the caller can decide whether
    /// to break the read loop immediately. Terminal errors from the wire are stored
    /// internally and surface via [`finalize`](Self::finalize).
    pub fn feed(
        &mut self,
        line: &str,
        tx: Option<&mpsc::Sender<RunnerEvent>>,
    ) -> Result<(), String> {
        if self.done {
            return Ok(());
        }

        let event: WireCodexEvent = serde_json::from_str(line).map_err(|e| {
            let detail = format!("Malformed JSON from codex CLI: {e}\nLine: {line}");
            self.error = Some(detail.clone());
            self.done = true;
            detail
        })?;

        tracing::trace!(line = %line, "codex --json raw line");

        match self.process_event(event, tx) {
            ParseAction::Continue => {}
            ParseAction::TurnCompleted => {
                self.done = true;
            }
            ParseAction::TurnError(detail) => {
                self.error = Some(detail);
                self.done = true;
            }
        }

        Ok(())
    }

    /// True when `turn.completed` (or a terminal error) has been processed, or when a JSON
    /// parse error has occurred. The caller should break the read loop when this returns `true`.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Take the error detail if the decoder encountered a terminal error or a JSON parse
    /// failure. The value is moved out — subsequent calls return `None`.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Consume the decoder into a [`RunResult`].
    ///
    /// Returns `Err` if the stream ended with an error event or without `turn.completed`.
    ///
    /// The caller should check [`take_error`](Self::take_error) and emit a
    /// `RunnerEvent::Error` **before** calling `finalize` if they want streaming consumers
    /// to see the error event.
    pub fn finalize(self) -> Result<RunResult, AilError> {
        if let Some(detail) = self.error {
            return Err(AilError::RunnerInvocationFailed {
                detail,
                context: None,
            });
        }
        if !self.done {
            return Err(AilError::RunnerInvocationFailed {
                detail: "Stream ended without a 'turn.completed' event".to_string(),
                context: None,
            });
        }
        Ok(RunResult {
            response: self.response,
            cost_usd: None, // not available in codex --json wire format
            session_id: self.thread_id,
            input_tokens: 0, // not available in codex --json wire format
            output_tokens: 0,
            thinking: if self.thinking.is_empty() {
                None
            } else {
                Some(self.thinking)
            },
            model: None,
            tool_events: self.tool_events,
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────────────────────

    /// Dispatch a parsed NDJSON event: emit [`RunnerEvent`]s, accumulate state, and return
    /// the [`ParseAction`] indicating whether the stream has reached a terminal state.
    fn process_event(
        &mut self,
        event: WireCodexEvent,
        tx: Option<&mpsc::Sender<RunnerEvent>>,
    ) -> ParseAction {
        match event.event_type.as_str() {
            "thread.started" => {
                self.thread_id = event.thread_id;
                tracing::debug!(thread_id = ?self.thread_id, "codex thread.started");
                ParseAction::Continue
            }

            "item.updated" => {
                let item_type = event.item_type.as_deref().unwrap_or("");
                let item = event.item.as_ref();
                tracing::debug!(item_type, "codex item.updated");

                match item_type {
                    "agent_message" => {
                        if let Some(text) = item.and_then(|i| i.text.as_deref()) {
                            if !text.is_empty() {
                                if let Some(tx) = tx {
                                    let _ = tx.send(RunnerEvent::StreamDelta {
                                        text: text.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    "reasoning" => {
                        if let Some(text) = item.and_then(|i| i.text.as_deref()) {
                            if !text.is_empty() {
                                // Accumulate
                                if !self.thinking.is_empty() {
                                    self.thinking.push('\n');
                                }
                                self.thinking.push_str(text);
                                // Emit
                                if let Some(tx) = tx {
                                    let _ = tx.send(RunnerEvent::Thinking {
                                        text: text.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    other => {
                        tracing::trace!(item_type = other, "codex item.updated: unhandled type");
                    }
                }
                ParseAction::Continue
            }

            "item.started" => {
                let item_type = event.item_type.as_deref().unwrap_or("");
                tracing::debug!(item_type, "codex item.started");

                if item_type == "command_execution" {
                    let item_id = event.item_id.clone().unwrap_or_default();
                    let command = event
                        .item
                        .as_ref()
                        .and_then(|i| i.command.as_deref())
                        .unwrap_or("");

                    // Accumulate the tool_call side
                    let input = serde_json::json!({ "command": command });
                    let content_json =
                        serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                    self.tool_events.push(ToolEvent {
                        event_type: "tool_call".to_string(),
                        tool_name: "Bash".to_string(),
                        tool_id: item_id.clone(),
                        content_json,
                        seq: self.tool_seq,
                    });
                    self.tool_seq += 1;

                    // Emit
                    if let Some(tx) = tx {
                        let _ = tx.send(RunnerEvent::ToolUse {
                            tool_name: "Bash".to_string(),
                            tool_use_id: Some(item_id),
                            input: Some(serde_json::json!({ "command": command })),
                        });
                    }
                }
                ParseAction::Continue
            }

            "item.completed" => {
                let item_type = event.item_type.as_deref().unwrap_or("");
                tracing::debug!(item_type, "codex item.completed");

                match item_type {
                    "command_execution" => {
                        let item_id = event.item_id.clone().unwrap_or_default();
                        let output = event
                            .item
                            .as_ref()
                            .and_then(|i| i.aggregated_output.as_deref())
                            .unwrap_or("");
                        let exit_code = event.item.as_ref().and_then(|i| i.exit_code);
                        let is_error = exit_code.map(|c| c != 0);

                        // Accumulate the tool_result side
                        self.tool_events.push(ToolEvent {
                            event_type: "tool_result".to_string(),
                            tool_name: String::new(),
                            tool_id: item_id.clone(),
                            content_json: output.to_string(),
                            seq: self.tool_seq,
                        });
                        self.tool_seq += 1;

                        // Emit
                        if let Some(tx) = tx {
                            let _ = tx.send(RunnerEvent::ToolResult {
                                tool_name: "Bash".to_string(),
                                tool_use_id: Some(item_id),
                                content: Some(output.to_string()),
                                is_error,
                            });
                        }
                    }
                    "agent_message" => {
                        // Final text — overwrite the accumulated response
                        let text = event
                            .item
                            .as_ref()
                            .and_then(|i| i.text.as_deref())
                            .unwrap_or("");
                        self.response = text.to_string();
                        tracing::debug!(
                            response_len = self.response.len(),
                            "codex agent_message completed"
                        );
                    }
                    other => {
                        tracing::trace!(item_type = other, "codex item.completed: unhandled type");
                    }
                }
                ParseAction::Continue
            }

            "turn.completed" => {
                tracing::debug!("codex turn.completed");
                ParseAction::TurnCompleted
            }

            "turn.failed" => {
                let detail = event
                    .error
                    .unwrap_or_else(|| "unknown error from codex CLI".to_string());
                tracing::debug!(detail = %detail, "codex turn.failed");
                ParseAction::TurnError(detail)
            }

            "error" => {
                let detail = event
                    .message
                    .unwrap_or_else(|| "unknown error from codex CLI".to_string());
                tracing::debug!(detail = %detail, "codex error event");
                ParseAction::TurnError(detail)
            }

            other => {
                tracing::trace!(event_type = other, "codex: unrecognized event type");
                ParseAction::Continue
            }
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn thread_started(thread_id: &str) -> String {
        serde_json::json!({
            "type": "thread.started",
            "thread_id": thread_id
        })
        .to_string()
    }

    fn item_updated_agent(text: &str) -> String {
        serde_json::json!({
            "type": "item.updated",
            "item_type": "agent_message",
            "item": { "text": text }
        })
        .to_string()
    }

    fn item_completed_agent(text: &str) -> String {
        serde_json::json!({
            "type": "item.completed",
            "item_type": "agent_message",
            "item": { "text": text }
        })
        .to_string()
    }

    fn item_updated_reasoning(text: &str) -> String {
        serde_json::json!({
            "type": "item.updated",
            "item_type": "reasoning",
            "item": { "text": text }
        })
        .to_string()
    }

    fn item_started_command(item_id: &str, command: &str) -> String {
        serde_json::json!({
            "type": "item.started",
            "item_type": "command_execution",
            "item_id": item_id,
            "item": { "command": command }
        })
        .to_string()
    }

    fn item_completed_command(item_id: &str, output: &str, exit_code: i32) -> String {
        serde_json::json!({
            "type": "item.completed",
            "item_type": "command_execution",
            "item_id": item_id,
            "item": { "aggregated_output": output, "exit_code": exit_code }
        })
        .to_string()
    }

    fn turn_completed() -> String {
        serde_json::json!({ "type": "turn.completed" }).to_string()
    }

    fn turn_failed(error: &str) -> String {
        serde_json::json!({ "type": "turn.failed", "error": error }).to_string()
    }

    fn error_event(message: &str) -> String {
        serde_json::json!({ "type": "error", "message": message }).to_string()
    }

    #[test]
    fn thread_started_sets_session_id() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&thread_started("thread-abc"), None).unwrap();
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.session_id.as_deref(), Some("thread-abc"));
    }

    #[test]
    fn agent_message_updated_emits_stream_delta() {
        let (tx, rx) = mpsc::channel();
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_updated_agent("hello world"), Some(&tx))
            .unwrap();
        dec.feed(&item_completed_agent("hello world"), Some(&tx))
            .unwrap();
        dec.feed(&turn_completed(), None).unwrap();

        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "hello world");

        let events: Vec<_> = rx.try_iter().collect();
        assert!(events
            .iter()
            .any(|e| matches!(e, RunnerEvent::StreamDelta { text } if text == "hello world")));
    }

    #[test]
    fn agent_message_completed_sets_response() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_completed_agent("final answer"), None)
            .unwrap();
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "final answer");
    }

    #[test]
    fn last_agent_message_wins() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_completed_agent("first"), None).unwrap();
        dec.feed(&item_completed_agent("second"), None).unwrap();
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "second");
    }

    #[test]
    fn reasoning_item_accumulated_to_thinking() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_updated_reasoning("step one"), None).unwrap();
        dec.feed(&item_updated_reasoning("step two"), None).unwrap();
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.thinking.as_deref(), Some("step one\nstep two"));
    }

    #[test]
    fn command_execution_emits_tool_use_and_result() {
        let (tx, rx) = mpsc::channel();
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_started_command("cmd-1", "ls -la"), Some(&tx))
            .unwrap();
        dec.feed(
            &item_completed_command("cmd-1", "total 0\ndrwxr-xr-x", 0),
            Some(&tx),
        )
        .unwrap();
        dec.feed(&turn_completed(), None).unwrap();

        let result = dec.finalize().unwrap();
        assert_eq!(result.tool_events.len(), 2);
        assert_eq!(result.tool_events[0].event_type, "tool_call");
        assert_eq!(result.tool_events[0].tool_name, "Bash");
        assert_eq!(result.tool_events[0].tool_id, "cmd-1");
        assert_eq!(result.tool_events[1].event_type, "tool_result");
        assert_eq!(result.tool_events[1].tool_id, "cmd-1");

        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.iter().any(|e| matches!(e, RunnerEvent::ToolUse {
            tool_name,
            tool_use_id: Some(id),
            ..
        } if tool_name == "Bash" && id == "cmd-1")));
        assert!(events.iter().any(|e| matches!(e, RunnerEvent::ToolResult {
            tool_name,
            tool_use_id: Some(id),
            is_error: Some(false),
            ..
        } if tool_name == "Bash" && id == "cmd-1")));
    }

    #[test]
    fn command_execution_non_zero_exit_marks_is_error() {
        let (tx, rx) = mpsc::channel();
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_started_command("cmd-2", "false"), Some(&tx))
            .unwrap();
        dec.feed(&item_completed_command("cmd-2", "", 1), Some(&tx))
            .unwrap();
        dec.feed(&turn_completed(), None).unwrap();
        dec.finalize().unwrap();

        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.iter().any(|e| matches!(
            e,
            RunnerEvent::ToolResult {
                is_error: Some(true),
                ..
            }
        )));
    }

    #[test]
    fn turn_completed_sets_done() {
        let mut dec = CodexNdjsonDecoder::new();
        assert!(!dec.is_done());
        dec.feed(&turn_completed(), None).unwrap();
        assert!(dec.is_done());
    }

    #[test]
    fn turn_failed_sets_error() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&turn_failed("rate limit exceeded"), None).unwrap();
        assert!(dec.is_done());
        assert_eq!(dec.take_error().as_deref(), Some("rate limit exceeded"));
    }

    #[test]
    fn error_event_sets_error() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&error_event("connection refused"), None).unwrap();
        assert!(dec.is_done());
        assert_eq!(dec.take_error().as_deref(), Some("connection refused"));
    }

    #[test]
    fn no_turn_completed_finalize_fails() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_completed_agent("partial"), None).unwrap();
        // Stream ends without turn.completed
        let err = dec.finalize().unwrap_err();
        assert!(err.detail().contains("turn.completed"));
    }

    #[test]
    fn malformed_json_sets_error() {
        let mut dec = CodexNdjsonDecoder::new();
        let result = dec.feed("not json {{", None);
        assert!(result.is_err());
        assert!(dec.is_done());
        assert!(dec.take_error().is_some());
    }

    #[test]
    fn feed_after_done_is_noop() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&item_completed_agent("first"), None).unwrap();
        dec.feed(&turn_completed(), None).unwrap();
        assert!(dec.is_done());
        // Second turn.completed should be silently ignored
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "first");
    }

    #[test]
    fn token_counts_always_zero() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.input_tokens, 0);
        assert_eq!(result.output_tokens, 0);
        assert!(result.cost_usd.is_none());
    }

    #[test]
    fn no_thread_started_session_id_is_none() {
        let mut dec = CodexNdjsonDecoder::new();
        dec.feed(&turn_completed(), None).unwrap();
        let result = dec.finalize().unwrap();
        assert!(result.session_id.is_none());
    }
}
