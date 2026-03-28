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
use std::os::unix::net::UnixStream;

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
                "tools": [{
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
                }]
            }
        }),

        "tools/call" => {
            let args = &request["params"]["arguments"];
            let tool_name = args["tool_name"].as_str().unwrap_or("").to_string();
            let tool_input = args["tool_input"].clone();

            match forward_to_socket(socket_path, &tool_name, &tool_input) {
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

        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": "Method not found" }
        }),
    }
}

/// Open a connection to the main ail process's Unix socket, send the permission request,
/// and read back the response.
fn forward_to_socket(socket_path: &str, tool_name: &str, tool_input: &Value) -> io::Result<Value> {
    let mut stream = UnixStream::connect(socket_path)?;

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
