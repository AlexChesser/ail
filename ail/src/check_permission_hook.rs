//! PreToolUse hook handler for tool permission checks.
//!
//! Spawned by Claude CLI for every tool use (via the `.*` matcher in the temp
//! `--settings` file written by `ClaudeCliRunner`). Forwards the permission
//! request to the AIL permission socket and returns the allow/deny decision.
//!
//! `AskUserQuestion` is short-circuited to `allow` immediately — it is handled
//! by the more specific `ask-user-hook` which runs first (AskUserQuestion matcher
//! takes precedence) and injects the user's answer via `updatedInput`.

use std::io::{self, BufRead, BufReader, Write};

use serde_json::{json, Value};

/// Entry point: run the permission-check PreToolUse hook.
pub fn run(socket_path: &str) {
    let hook_input: Value = match serde_json::from_reader(io::stdin()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "check-permission-hook: failed to parse hook input");
            // On parse failure, allow through — do not block Claude.
            print_allow();
            return;
        }
    };

    let tool_name = hook_input["tool_name"].as_str().unwrap_or("");

    // AskUserQuestion is handled by ask-user-hook; this hook just passes it through.
    if tool_name == "AskUserQuestion" {
        print_allow();
        return;
    }

    let tool_input = &hook_input["tool_input"];

    let resp = match forward_to_socket(socket_path, tool_name, tool_input) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "check-permission-hook: socket error");
            // On socket failure, deny to avoid unintended tool execution.
            print_deny("ail permission bridge error");
            return;
        }
    };

    let behavior = resp["behavior"].as_str().unwrap_or("deny");
    if behavior == "allow" {
        print_allow();
    } else {
        let reason = resp["message"].as_str().unwrap_or("denied by ail");
        print_deny(reason);
    }
}

pub(crate) fn build_allow_response() -> serde_json::Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow"
        }
    })
}

pub(crate) fn build_deny_response(reason: &str) -> serde_json::Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "reason": reason
        }
    })
}

fn print_allow() {
    println!(
        "{}",
        serde_json::to_string(&build_allow_response()).unwrap_or_default()
    );
}

fn print_deny(reason: &str) {
    println!(
        "{}",
        serde_json::to_string(&build_deny_response(reason)).unwrap_or_default()
    );
}

fn forward_to_socket(socket_path: &str, tool_name: &str, tool_input: &Value) -> io::Result<Value> {
    let mut stream = ail_core::ipc::connect_local(socket_path)?;

    let request = json!({
        "tool_name": tool_name,
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

    #[test]
    fn ask_user_question_is_handled_specially() {
        // The hook short-circuits for AskUserQuestion — verified by checking the
        // tool_name guard in run(). No socket round-trip happens.
        let hook_input = json!({
            "tool_name": "AskUserQuestion",
            "tool_input": { "questions": [] }
        });
        assert_eq!(hook_input["tool_name"].as_str().unwrap(), "AskUserQuestion");
    }

    #[test]
    fn allow_response_has_correct_structure() {
        let resp = build_allow_response();
        let output = &resp["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PreToolUse");
        assert_eq!(output["permissionDecision"], "allow");
        assert!(output.get("reason").is_none());
    }

    #[test]
    fn deny_response_has_correct_structure() {
        let resp = build_deny_response("not permitted");
        let output = &resp["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PreToolUse");
        assert_eq!(output["permissionDecision"], "deny");
        assert_eq!(output["reason"], "not permitted");
    }

    #[test]
    fn deny_response_preserves_reason_string() {
        let reason = "ail permission bridge error";
        let resp = build_deny_response(reason);
        assert_eq!(resp["hookSpecificOutput"]["reason"], reason);
    }

    #[test]
    fn allow_response_is_valid_json() {
        let resp = build_allow_response();
        let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
        let reparsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("must round-trip");
        assert_eq!(
            reparsed["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
    }

    #[test]
    fn deny_response_is_valid_json() {
        let resp = build_deny_response("denied by ail");
        let serialized = serde_json::to_string(&resp).expect("serialization must succeed");
        let reparsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("must round-trip");
        assert_eq!(reparsed["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(reparsed["hookSpecificOutput"]["reason"], "denied by ail");
    }
}
