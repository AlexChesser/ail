//! PreToolUse hook handler for `AskUserQuestion`.
//!
//! Spawned by Claude CLI for every `AskUserQuestion` tool call (via the `AskUserQuestion`
//! matcher in the temp `--settings` file written by `ClaudeCliRunner`).
//!
//! Protocol:
//! 1. Claude CLI writes the hook payload JSON to this process's stdin.
//! 2. This hook normalises the raw `tool_input` (handling all four model format variants).
//! 3. Forwards the canonical payload to the AIL permission socket.
//! 4. Blocks until the user answers (the socket server calls the `PermissionResponder`
//!    callback, which blocks until the extension delivers a response).
//! 5. Writes a `hookSpecificOutput` JSON line to stdout containing `updatedInput` with
//!    the user's answer in the `answers` map.
//! 6. Exits 0 — Claude CLI reads the answer from `updatedInput.answers` and continues.

use std::io::{self, BufRead, BufReader, Write};

use serde_json::{json, Value};

/// Entry point: run the AskUserQuestion PreToolUse hook.
pub fn run(socket_path: &str) {
    let hook_input: Value = match serde_json::from_reader(io::stdin()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "ask-user-hook: failed to parse hook input from stdin");
            print_allow_passthrough();
            return;
        }
    };

    let tool_input = &hook_input["tool_input"];
    let normalized = crate::ask_user_types::parse(tool_input);

    let answer = match forward_to_socket(socket_path, &normalized) {
        Ok(resp) => resp["message"].as_str().unwrap_or("").to_string(),
        Err(e) => {
            tracing::error!(error = %e, "ask-user-hook: socket error");
            print_allow_passthrough();
            return;
        }
    };

    // Build { question_text → answer } for each question in the normalized payload.
    let answers = build_answers_map(&normalized, &answer);

    // updatedInput = normalized questions + answers map.
    let mut updated_input = normalized.clone();
    if let Some(obj) = updated_input.as_object_mut() {
        obj.insert("answers".to_string(), answers);
    }

    let response = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated_input
        }
    });

    println!("{}", serde_json::to_string(&response).unwrap_or_default());
}

pub(crate) fn build_answers_map(normalized: &Value, answer: &str) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(questions) = normalized["questions"].as_array() {
        for q in questions {
            if let Some(question_text) = q["question"].as_str() {
                map.insert(question_text.to_string(), Value::String(answer.to_string()));
            }
        }
    }
    Value::Object(map)
}

pub(crate) fn build_passthrough_response() -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow"
        }
    })
}

fn print_allow_passthrough() {
    println!(
        "{}",
        serde_json::to_string(&build_passthrough_response()).unwrap_or_default()
    );
}

fn forward_to_socket(socket_path: &str, tool_input: &Value) -> io::Result<Value> {
    let mut stream = ail_core::ipc::connect_local(socket_path)?;

    let request = json!({
        "tool_name": "AskUserQuestion",
        "tool_input": tool_input
    });
    let mut req_line = serde_json::to_string(&request).map_err(io::Error::other)?;
    req_line.push('\n');
    stream.write_all(req_line.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line)?;

    serde_json::from_str(resp_line.trim())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // ── Parse-chain integration tests ────────────────────────────────────────
    // These tests validate the full normaliser chain (migrated from mcp_bridge.rs).
    // Per-format unit tests live in the individual `ask_user_types` files.

    fn questions(v: &Value) -> &Vec<Value> {
        v["questions"].as_array().expect("questions array")
    }

    #[test]
    fn chain_routes_canonical_format() {
        let input = json!({
            "questions": [{
                "header": "Pick one",
                "question": "Which color?",
                "multiSelect": false,
                "options": [
                    { "label": "Red", "description": "Warm" },
                    { "label": "Blue" }
                ]
            }]
        });
        let out = crate::ask_user_types::parse(&input);
        let qs = questions(&out);
        assert_eq!(qs[0]["question"], "Which color?");
        assert_eq!(qs[0]["options"][0]["description"], "Warm");
        assert!(qs[0]["options"][1].get("description").is_none());
    }

    #[test]
    fn chain_routes_claude_preview_format() {
        let input = json!({
            "questions": [{
                "question": "Favorite color?",
                "options": [
                    { "label": "Red",   "preview": "Red"   },
                    { "label": "Blue",  "preview": "Blue"  },
                    { "label": "Green", "preview": "Green" }
                ]
            }]
        });
        let out = crate::ask_user_types::parse(&input);
        let opts = &questions(&out)[0]["options"];
        assert_eq!(opts[0]["label"], "Red");
        assert_eq!(opts[0]["description"], "Red");
        assert_eq!(opts[1]["description"], "Blue");
        assert_eq!(opts[2]["description"], "Green");
    }

    #[test]
    fn chain_routes_flat_format() {
        let input = json!({
            "question": "Proceed?",
            "options": [{ "label": "Yes" }, { "label": "No" }]
        });
        let out = crate::ask_user_types::parse(&input);
        let qs = questions(&out);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0]["question"], "Proceed?");
    }

    #[test]
    fn chain_routes_stringified_format() {
        let qs_str = serde_json::to_string(&json!([{
            "question": "What?",
            "options": [{ "label": "A" }]
        }]))
        .unwrap();
        let input = json!({ "questions": qs_str });
        let out = crate::ask_user_types::parse(&input);
        assert_eq!(questions(&out)[0]["question"], "What?");
    }

    #[test]
    fn chain_falls_back_to_empty_on_unrecognised_input() {
        let input = json!({ "unrelated": "data" });
        let out = crate::ask_user_types::parse(&input);
        assert!(questions(&out).is_empty());
    }

    #[test]
    fn build_answers_map_uses_question_text_as_key() {
        let normalized = json!({
            "questions": [{ "question": "Which color?", "options": [] }]
        });
        let answers = build_answers_map(&normalized, "Red");
        assert_eq!(answers["Which color?"], "Red");
    }

    #[test]
    fn build_answers_map_handles_multiple_questions() {
        let normalized = json!({
            "questions": [
                { "question": "First?", "options": [] },
                { "question": "Second?", "options": [] }
            ]
        });
        // Same answer for both (single PermissionResponse::Deny carries one string).
        let answers = build_answers_map(&normalized, "Yes");
        assert_eq!(answers["First?"], "Yes");
        assert_eq!(answers["Second?"], "Yes");
    }

    #[test]
    fn build_answers_map_handles_empty_questions() {
        let normalized = json!({ "questions": [] });
        let answers = build_answers_map(&normalized, "whatever");
        assert!(answers.as_object().unwrap().is_empty());
    }

    #[test]
    fn build_answers_map_handles_missing_questions_field() {
        let normalized = json!({ "unrelated": "data" });
        let answers = build_answers_map(&normalized, "something");
        assert!(answers.as_object().unwrap().is_empty());
    }

    #[test]
    fn build_answers_map_skips_questions_without_question_text() {
        let normalized = json!({
            "questions": [
                { "options": [{ "label": "A" }] },
                { "question": "Real question?", "options": [] }
            ]
        });
        let answers = build_answers_map(&normalized, "answer");
        let obj = answers.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(answers["Real question?"], "answer");
    }

    #[test]
    fn passthrough_response_has_allow_decision() {
        let resp = build_passthrough_response();
        let output = &resp["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PreToolUse");
        assert_eq!(output["permissionDecision"], "allow");
    }

    #[test]
    fn passthrough_response_has_no_updated_input() {
        let resp = build_passthrough_response();
        assert!(resp["hookSpecificOutput"].get("updatedInput").is_none());
    }

    #[test]
    fn passthrough_response_is_valid_json() {
        let resp = build_passthrough_response();
        let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
        let reparsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("must round-trip");
        assert_eq!(
            reparsed["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
    }
}
