//! Stateful decoder for the Claude CLI `--output-format stream-json` wire format.
//!
//! [`ClaudeNdjsonDecoder`] accepts NDJSON lines from the claude CLI one at a time via
//! [`ClaudeNdjsonDecoder::feed`]. It emits [`RunnerEvent`]s for streaming consumers and
//! accumulates state needed to build the final [`RunResult`] when the terminal `result`
//! event arrives. Decoding is completely decoupled from process lifecycle — these functions
//! can be tested with raw byte strings, with no subprocess involved.

use std::sync::mpsc;

use super::super::{RunResult, RunnerEvent, ToolEvent};
use super::wire_dto::{WireContentBlock, WireUsage};
use crate::error::AilError;

// ── Internal parse action ──────────────────────────────────────────────────────────────────────

/// Terminal outcome of processing a single NDJSON event.
enum ParseAction {
    /// Non-terminal event — any `RunnerEvent`s were already sent through `tx`.
    Continue,
    /// An `assistant` event carried token usage — caller accumulates into running totals.
    TokensObserved { input: u64, output: u64 },
    /// The `result` event arrived with a successful response.
    ResultReceived {
        response: Option<String>,
        cost_usd: Option<f64>,
        session_id: Option<String>,
        model: Option<String>,
    },
    /// The `result` event arrived indicating an error.
    ResultError(String),
}

// ── Decoder ───────────────────────────────────────────────────────────────────────────────────

/// Accumulates state from a stream of Claude CLI NDJSON events.
///
/// Call [`feed`](ClaudeNdjsonDecoder::feed) for each non-empty line from the claude CLI
/// stdout. When [`is_done`](ClaudeNdjsonDecoder::is_done) returns `true`, call
/// [`finalize`](ClaudeNdjsonDecoder::finalize) to consume the decoder into a [`RunResult`].
///
/// If the `result` event contained an error, [`finalize`] returns
/// [`AilError::RunnerInvocationFailed`]. If the stream ended without a `result` event,
/// [`finalize`] also returns an error.
#[derive(Default)]
pub struct ClaudeNdjsonDecoder {
    response: Option<String>,
    cost_usd: Option<f64>,
    session_id: Option<String>,
    model: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    thinking: String,
    tool_events: Vec<ToolEvent>,
    tool_seq: i64,
    /// Set by a `ResultError` event or a JSON parse failure.
    error: Option<String>,
    /// True when a terminal `result` event (success or error) has been processed.
    done: bool,
}

impl ClaudeNdjsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one NDJSON line from the claude CLI stdout.
    ///
    /// Emits zero or more [`RunnerEvent`]s on `tx` (when `Some`) for streaming consumers.
    /// Sets [`is_done`](Self::is_done) to `true` when a terminal `result` event is seen.
    ///
    /// On a JSON parse error the internal error is set, `is_done()` becomes `true`, and
    /// no further calls to `feed` will have any effect.
    ///
    /// Returns `Err(detail)` only for JSON parse failures, so the caller can decide whether
    /// to break the read loop immediately. [`ResultError`] from the wire is stored internally
    /// and surfaces via [`finalize`](Self::finalize).
    pub fn feed(
        &mut self,
        line: &str,
        tx: Option<&mpsc::Sender<RunnerEvent>>,
    ) -> Result<(), String> {
        if self.done {
            return Ok(());
        }

        let event: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            let detail = format!("Malformed JSON from claude CLI: {e}\nLine: {line}");
            self.error = Some(detail.clone());
            self.done = true;
            detail
        })?;

        tracing::trace!(line = %line, "stream-json raw line");

        match self.process_event(&event, tx) {
            ParseAction::Continue => {}
            ParseAction::TokensObserved { input, output } => {
                self.input_tokens = self.input_tokens.saturating_add(input);
                self.output_tokens = self.output_tokens.saturating_add(output);
            }
            ParseAction::ResultReceived {
                response,
                cost_usd,
                session_id,
                model,
            } => {
                self.response = response;
                self.cost_usd = cost_usd;
                self.session_id = session_id;
                self.model = model;
                self.done = true;
            }
            ParseAction::ResultError(detail) => {
                self.error = Some(detail);
                self.done = true;
            }
        }

        Ok(())
    }

    /// True when a terminal `result` event (success or error) has been processed, or when a
    /// JSON parse error has occurred. The caller should break the read loop when this returns
    /// `true`.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Take the error detail if the decoder encountered a `ResultError` or a JSON parse
    /// failure. The value is moved out — subsequent calls return `None`.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Consume the decoder into a [`RunResult`].
    ///
    /// Returns `Err` if the `result` event was an error (see [`take_error`](Self::take_error))
    /// or if the stream ended without a terminal `result` event.
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
        let response = self
            .response
            .ok_or_else(|| AilError::RunnerInvocationFailed {
                detail: "Stream ended without a 'result' event".to_string(),
                context: None,
            })?;
        Ok(RunResult {
            response,
            cost_usd: self.cost_usd,
            session_id: self.session_id,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            thinking: if self.thinking.is_empty() {
                None
            } else {
                Some(self.thinking)
            },
            model: self.model,
            tool_events: self.tool_events,
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────────────────────

    /// Process a single parsed NDJSON event: emit [`RunnerEvent`]s, accumulate thinking and
    /// tool events, and return the terminal [`ParseAction`].
    ///
    /// This is a single pass over each content block — accumulation and emission happen
    /// together rather than in separate traversals.
    fn process_event(
        &mut self,
        event: &serde_json::Value,
        tx: Option<&mpsc::Sender<RunnerEvent>>,
    ) -> ParseAction {
        let event_type = event["type"].as_str().unwrap_or("");

        match event_type {
            "assistant" => {
                let blocks = self.parse_content_blocks(&event["message"]["content"]);
                let block_types: Vec<&str> = blocks.iter().map(|b| b.block_type.as_str()).collect();
                tracing::debug!(event_type, ?block_types, "stream-json assistant event");

                // Single pass: accumulate and emit for all content block types.
                for block in blocks {
                    match block.block_type.as_str() {
                        "text" => {
                            let text = block.text.unwrap_or_default();
                            if !text.is_empty() {
                                if let Some(tx) = tx {
                                    let _ = tx.send(RunnerEvent::StreamDelta { text });
                                }
                            }
                        }
                        "thinking" => {
                            let text = block.thinking.unwrap_or_default();
                            if !text.is_empty() {
                                // Accumulate
                                if !self.thinking.is_empty() {
                                    self.thinking.push('\n');
                                }
                                self.thinking.push_str(&text);
                                // Emit
                                if let Some(tx) = tx {
                                    let _ = tx.send(RunnerEvent::Thinking { text });
                                }
                            }
                        }
                        "tool_use" => {
                            let name = block.name.unwrap_or_default();
                            let id = block.id.unwrap_or_default();
                            let input = block.input;
                            // Accumulate
                            let content_json = input
                                .as_ref()
                                .map(|v| {
                                    serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string())
                                })
                                .unwrap_or_else(|| "{}".to_string());
                            self.tool_events.push(ToolEvent {
                                event_type: "tool_call".to_string(),
                                tool_name: name.clone(),
                                tool_id: id.clone(),
                                content_json,
                                seq: self.tool_seq,
                            });
                            self.tool_seq += 1;
                            // Emit
                            if let Some(tx) = tx {
                                let _ = tx.send(RunnerEvent::ToolUse {
                                    tool_name: name,
                                    tool_use_id: Some(id),
                                    input,
                                });
                            }
                        }
                        other => {
                            tracing::debug!(
                                block_type = other,
                                "stream-json: unrecognized assistant content block type"
                            );
                        }
                    }
                }

                let usage: WireUsage = serde_json::from_value(event["message"]["usage"].clone())
                    .unwrap_or_default();
                if usage.input_tokens > 0 || usage.output_tokens > 0 {
                    tracing::debug!(
                        event_type,
                        input_tokens = usage.input_tokens,
                        output_tokens = usage.output_tokens,
                        "stream-json assistant event with usage"
                    );
                    return ParseAction::TokensObserved {
                        input: usage.input_tokens,
                        output: usage.output_tokens,
                    };
                }

                ParseAction::Continue
            }

            "user" => {
                let blocks = self.parse_content_blocks(&event["message"]["content"]);
                for block in blocks {
                    if block.block_type != "tool_result" {
                        continue;
                    }
                    let tool_use_id = block.tool_use_id.unwrap_or_default();
                    let raw_content = block.content;
                    // Accumulate: content_json for storage
                    let content_json = match &raw_content {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(other) => {
                            serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string())
                        }
                        None => "{}".to_string(),
                    };
                    self.tool_events.push(ToolEvent {
                        event_type: "tool_result".to_string(),
                        tool_name: String::new(),
                        tool_id: tool_use_id.clone(),
                        content_json,
                        seq: self.tool_seq,
                    });
                    self.tool_seq += 1;
                    // Emit: content as string for RunnerEvent
                    if let Some(tx) = tx {
                        let content_str = raw_content
                            .as_ref()
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                raw_content
                                    .as_ref()
                                    .and_then(|v| v.as_object().map(|_| ""))
                            })
                            .map(str::to_string);
                        let _ = tx.send(RunnerEvent::ToolResult {
                            tool_name: block.tool_name.unwrap_or_default(),
                            tool_use_id: Some(tool_use_id),
                            content: content_str,
                            is_error: block.is_error,
                        });
                    }
                }
                tracing::debug!(event_type, "stream-json user event");
                ParseAction::Continue
            }

            "result" => {
                let subtype = event["subtype"].as_str().unwrap_or("");
                let is_error = subtype == "error" || event["is_error"].as_bool().unwrap_or(false);
                let result_len = event["result"].as_str().map(|s| s.len());
                let cost = event["total_cost_usd"].as_f64();
                let session_id = event["session_id"].as_str();
                let model = event["model"].as_str();
                tracing::debug!(
                    event_type,
                    subtype,
                    is_error,
                    result_len,
                    has_cost = cost.is_some(),
                    has_session_id = session_id.is_some(),
                    has_model = model.is_some(),
                    "stream-json result event"
                );
                if is_error {
                    ParseAction::ResultError(
                        event["result"]
                            .as_str()
                            .unwrap_or("unknown error from claude CLI")
                            .to_string(),
                    )
                } else {
                    ParseAction::ResultReceived {
                        response: event["result"].as_str().map(str::to_string),
                        cost_usd: cost,
                        session_id: session_id.map(str::to_string),
                        model: model.map(str::to_string),
                    }
                }
            }

            "system" => {
                tracing::debug!(event_type, "stream-json system event");
                ParseAction::Continue
            }

            other => {
                tracing::warn!(event_type = other, "unexpected stream-json event type");
                ParseAction::Continue
            }
        }
    }

    /// Deserialize the `content` array from an assistant or user message into typed blocks.
    /// Returns an empty vec if the value is not an array or deserialization fails.
    fn parse_content_blocks(&self, content: &serde_json::Value) -> Vec<WireContentBlock> {
        content
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_event(text: &str) -> String {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": text}],
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        })
        .to_string()
    }

    fn make_result_event(response: &str) -> String {
        serde_json::json!({
            "type": "result",
            "subtype": "success",
            "result": response,
            "total_cost_usd": 0.001,
            "session_id": "sess-123",
            "model": "claude-opus-4-6"
        })
        .to_string()
    }

    fn make_error_result_event(detail: &str) -> String {
        serde_json::json!({
            "type": "result",
            "subtype": "error",
            "is_error": true,
            "result": detail
        })
        .to_string()
    }

    fn make_tool_use_event(name: &str, id: &str) -> String {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "name": name,
                    "id": id,
                    "input": {"path": "/foo"}
                }],
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }
        })
        .to_string()
    }

    fn make_tool_result_event(tool_use_id: &str, content: &str) -> String {
        serde_json::json!({
            "type": "user",
            "message": {
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content
                }]
            }
        })
        .to_string()
    }

    fn make_thinking_event(thinking: &str) -> String {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "thinking", "thinking": thinking}],
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        })
        .to_string()
    }

    #[test]
    fn empty_result_event_succeeds() {
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_result_event(""), None).unwrap();
        assert!(dec.is_done());
        assert!(dec.take_error().is_none());
        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "");
        assert_eq!(result.cost_usd, Some(0.001));
        assert_eq!(result.session_id.as_deref(), Some("sess-123"));
        assert_eq!(result.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn text_delta_emits_stream_event_and_finalizes() {
        let (tx, rx) = mpsc::channel();
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_text_event("hello world"), Some(&tx))
            .unwrap();
        assert!(!dec.is_done());
        dec.feed(&make_result_event("hello world"), Some(&tx))
            .unwrap();
        assert!(dec.is_done());

        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "hello world");

        let events: Vec<_> = rx.try_iter().collect();
        assert!(events
            .iter()
            .any(|e| matches!(e, RunnerEvent::StreamDelta { text } if text == "hello world")));
        // Completed is emitted by the caller (invoke_streaming), not by feed().
        assert!(!events
            .iter()
            .any(|e| matches!(e, RunnerEvent::Completed(_))));
    }

    #[test]
    fn result_error_is_captured() {
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_error_result_event("something went wrong"), None)
            .unwrap();
        assert!(dec.is_done());
        assert_eq!(dec.take_error().as_deref(), Some("something went wrong"));
        // After taking the error, finalize returns Ok but with empty response — but the
        // caller would have already returned early after take_error() was Some.
    }

    #[test]
    fn malformed_json_sets_error() {
        let mut dec = ClaudeNdjsonDecoder::new();
        let result = dec.feed("not json at all {{", None);
        assert!(result.is_err());
        assert!(dec.is_done());
        assert!(dec.take_error().is_some());
    }

    #[test]
    fn no_result_event_finalize_fails() {
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_text_event("partial"), None).unwrap();
        // Stream ends without a result event.
        let err = dec.finalize().unwrap_err();
        assert!(err.detail().contains("result"));
    }

    #[test]
    fn tool_use_and_result_accumulated() {
        let (tx, _rx) = mpsc::channel();
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_tool_use_event("Read", "tu-1"), Some(&tx))
            .unwrap();
        dec.feed(&make_tool_result_event("tu-1", "file contents"), Some(&tx))
            .unwrap();
        dec.feed(&make_result_event("done"), Some(&tx)).unwrap();

        let result = dec.finalize().unwrap();
        assert_eq!(result.tool_events.len(), 2);
        assert_eq!(result.tool_events[0].event_type, "tool_call");
        assert_eq!(result.tool_events[0].tool_name, "Read");
        assert_eq!(result.tool_events[1].event_type, "tool_result");
    }

    #[test]
    fn token_counts_accumulated_across_events() {
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_tool_use_event("Bash", "tu-2"), None)
            .unwrap();
        dec.feed(&make_result_event("ok"), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.input_tokens, 10);
        assert_eq!(result.output_tokens, 5);
    }

    #[test]
    fn thinking_block_accumulated() {
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_thinking_event("step 1"), None).unwrap();
        dec.feed(&make_thinking_event("step 2"), None).unwrap();
        dec.feed(&make_result_event("answer"), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.thinking.as_deref(), Some("step 1\nstep 2"));
    }

    #[test]
    fn completed_event_emitted_on_feed_result() {
        let (tx, rx) = mpsc::channel();
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_result_event("response text"), Some(&tx))
            .unwrap();
        // Note: Completed is emitted by the *caller* (invoke_streaming), not by the decoder.
        // The decoder only emits StreamDelta / Thinking / ToolUse / ToolResult.
        // So there should be NO Completed event from feed alone.
        let events: Vec<_> = rx.try_iter().collect();
        assert!(!events
            .iter()
            .any(|e| matches!(e, RunnerEvent::Completed(_))));
    }

    #[test]
    fn feed_after_done_is_noop() {
        let mut dec = ClaudeNdjsonDecoder::new();
        dec.feed(&make_result_event("first"), None).unwrap();
        assert!(dec.is_done());
        // A second result should be silently ignored.
        dec.feed(&make_result_event("second"), None).unwrap();
        let result = dec.finalize().unwrap();
        assert_eq!(result.response, "first");
    }
}
