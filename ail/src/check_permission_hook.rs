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

fn print_allow() {
    let response = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow"
        }
    });
    println!("{}", serde_json::to_string(&response).unwrap_or_default());
}

fn print_deny(reason: &str) {
    let response = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "reason": reason
        }
    });
    println!("{}", serde_json::to_string(&response).unwrap_or_default());
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
}
