//! Minimal MCP server over stdio that bridges tool permission requests to the main ail process.
//!
//! Claude CLI spawns this as a subprocess (via the MCP config written by `ClaudeCliRunner`)
//! and communicates with it using JSON-RPC 2.0 over newline-delimited stdin/stdout.
//!
//! For each `tools/call` request, this bridge:
//! 1. Opens a connection to the Unix socket at `--socket <path>`.
//! 2. Sends the tool name and input as a JSON line.
//! 3. Reads the permission decision (allow/deny) as a JSON line.
//! 4. Returns the decision to Claude CLI as an MCP tool result.

use std::io::{self, BufRead, BufReader, Write};

use serde_json::{json, Value};

/// Entry point: run the MCP bridge, reading JSON-RPC from stdin, writing to stdout.
pub fn run(socket_path: &str) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "mcp-bridge: malformed JSON-RPC line");
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request["method"].as_str().unwrap_or("");

        // Notifications have no `id` and require no response.
        let is_notification = method.starts_with("notifications/");
        if is_notification {
            continue;
        }

        let response = handle_request(method, &request, socket_path, id);
        let mut line_out = serde_json::to_string(&response).unwrap_or_default();
        line_out.push('\n');
        let _ = stdout.write_all(line_out.as_bytes());
        let _ = stdout.flush();
    }
}

fn handle_request(method: &str, request: &Value, socket_path: &str, id: Value) -> Value {
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "ail-permission-bridge", "version": "0.1.0" }
            }
        }),

        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "ail_check_permission",
                        "description": "Checks with the ail TUI whether a tool use is permitted.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "tool_name": {
                                    "type": "string",
                                    "description": "The tool that needs permission."
                                },
                                "tool_input": {
                                    "description": "The arguments Claude would pass to the tool."
                                }
                            },
                            "required": ["tool_name", "tool_input"]
                        }
                    },
                    {
                        "name": "ail_ask_user",
                        "description": "Ask the user a question with optional multiple-choice options. Use this instead of AskUserQuestion to ask the human for input.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "question": {
                                    "type": "string",
                                    "description": "The question to ask the user."
                                },
                                "header": {
                                    "description": "Optional title or header for the question."
                                },
                                "multiSelect": {
                                    "description": "Whether multiple options can be selected (boolean or string)."
                                },
                                "options": {
                                    "description": "Array of options: strings, {label} objects, {label, description} objects, or a JSON-encoded array string."
                                },
                                "questions": {
                                    "description": "Alternative: array of question objects, each with {header, question, multiSelect, options}. Use instead of the flat fields above for multiple questions."
                                }
                            },
                            "required": []
                        }
                    }
                ]
            }
        }),

        "tools/call" => {
            // Route by MCP tool name.
            let called_tool = request["params"]["name"].as_str().unwrap_or("");
            match called_tool {
                "ail_ask_user" => {
                    // Normalize the lenient input and forward as a native AskUserQuestion event.
                    let args = &request["params"]["arguments"];
                    let normalized = normalize_ask_user_input(args);
                    match forward_to_socket(socket_path, "AskUserQuestion", &normalized) {
                        Ok(resp_json) => {
                            // The permission socket returns {"behavior":"deny","message":"<answer>"}.
                            // Return the user's answer as the clean tool result text.
                            let answer = resp_json["message"].as_str().unwrap_or("").to_string();
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": answer
                                    }]
                                }
                            })
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "mcp-bridge: socket error (ail_ask_user)");
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": "ail permission bridge error"
                                    }]
                                }
                            })
                        }
                    }
                }
                _ => {
                    // ail_check_permission (and any unrecognised tool): existing permission-bridge behaviour.
                    // Claude CLI sends: {tool_name, input, tool_use_id}
                    // Note: the field is "input", not "tool_input".
                    let args = &request["params"]["arguments"];
                    let tool_name = args["tool_name"].as_str().unwrap_or("").to_string();
                    let input = args["input"].clone();

                    match forward_to_socket(socket_path, &tool_name, &input) {
                        Ok(resp_json) => {
                            // MCP tool result: content is a text block containing the JSON response.
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": serde_json::to_string(&resp_json).unwrap_or_default()
                                    }]
                                }
                            })
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "mcp-bridge: socket error");
                            // On socket failure, deny the tool to avoid hanging Claude CLI.
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": "{\"behavior\":\"deny\",\"message\":\"ail permission bridge error\"}"
                                    }]
                                }
                            })
                        }
                    }
                }
            }
        }

        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "Method not found" }
        }),
    }
}

/// Normalise a model-produced `ail_ask_user` input into the canonical
/// `{ questions: [{ header, question, multiSelect, options: [{ label, description? }] }] }`
/// format before forwarding to the permission socket as an `AskUserQuestion` event.
///
/// Delegates to `crate::ask_user_types::parse`, which tries each known format in order
/// and falls back gracefully. See `ail/src/ask_user_types/` to add support for new formats.
fn normalize_ask_user_input(input: &Value) -> Value {
    crate::ask_user_types::parse(input)
}

/// Open a connection to the main ail process's Unix socket, send the permission request,
/// and read back the response.
fn forward_to_socket(socket_path: &str, tool_name: &str, tool_input: &Value) -> io::Result<Value> {
    let mut stream = ail_core::ipc::connect_local(socket_path)?;

    // Write the request as a single JSON line.
    let request = json!({
        "tool_name": tool_name,
        "tool_input": tool_input
    });
    let mut req_line = serde_json::to_string(&request).map_err(io::Error::other)?;
    req_line.push('\n');
    stream.write_all(req_line.as_bytes())?;

    // Read the response line.
    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line)?;

    serde_json::from_str(resp_line.trim())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration tests for the parse chain via `normalize_ask_user_input`.
    /// Per-format unit tests live in the individual `ask_user_types` files.

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
        let out = normalize_ask_user_input(&input);
        let qs = questions(&out);
        assert_eq!(qs[0]["question"], "Which color?");
        assert_eq!(qs[0]["options"][0]["description"], "Warm");
        assert!(qs[0]["options"][1].get("description").is_none());
    }

    #[test]
    fn chain_routes_claude_preview_format() {
        // The runlog format: preview instead of description — should be remapped.
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
        let out = normalize_ask_user_input(&input);
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
        let out = normalize_ask_user_input(&input);
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
        let out = normalize_ask_user_input(&input);
        assert_eq!(questions(&out)[0]["question"], "What?");
    }

    #[test]
    fn chain_falls_back_to_empty_questions_on_unrecognised_input() {
        let input = json!({ "unrelated": "data" });
        let out = normalize_ask_user_input(&input);
        assert!(questions(&out).is_empty());
    }
}
