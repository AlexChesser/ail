# r10. AIL Runner Plugin Protocol

> **Status:** alpha — protocol version `1`

---

## Purpose

This document defines the **AIL Runner Plugin Protocol** — a JSON-RPC 2.0 protocol over stdin/stdout that enables third-party runner extensions to work with ail without recompilation.

A runner plugin is a standalone executable. ail spawns it as a subprocess, sends JSON-RPC requests on its stdin, and reads JSON-RPC responses and notifications from its stdout. This is the same transport pattern used by MCP (Model Context Protocol) and LSP (Language Server Protocol).

---

## Transport

- **stdin (ail → plugin):** Newline-delimited JSON. Each line is a complete JSON-RPC 2.0 request or notification.
- **stdout (plugin → ail):** Newline-delimited JSON. Each line is a complete JSON-RPC 2.0 response or notification.
- **stderr:** Diagnostic output only. ail captures stderr for logging but does not parse it as protocol messages.
- **Encoding:** UTF-8.
- **Line terminator:** `\n` (LF). Plugins should accept `\r\n` but emit `\n`.

---

## JSON-RPC 2.0 Subset

The protocol uses the JSON-RPC 2.0 specification with these conventions:

- **Requests** have `jsonrpc`, `id`, `method`, and optional `params`.
- **Responses** have `jsonrpc`, `id`, and either `result` or `error`.
- **Notifications** have `jsonrpc` and `method` but no `id`. They are fire-and-forget.
- **Batch requests** are not supported.

---

## Lifecycle

```
ail                              plugin
 │                                  │
 │── spawn ──────────────────────►  │
 │                                  │
 │── initialize ─────────────────►  │
 │◄─ initialize response ────────  │
 │                                  │
 │── invoke ─────────────────────►  │
 │◄─ stream/delta (notification) ─  │  (repeated)
 │◄─ stream/thinking (notif.) ────  │  (optional)
 │◄─ stream/tool_use (notif.) ────  │  (optional)
 │◄─ stream/permission_request ───  │  (optional)
 │── permission/respond ─────────►  │  (if permission requested)
 │◄─ stream/tool_result (notif.) ─  │  (optional)
 │◄─ stream/cost_update (notif.) ─  │  (optional)
 │◄─ invoke response ────────────  │
 │                                  │
 │── shutdown ───────────────────►  │
 │◄─ shutdown response ──────────  │
 │                                  │
 │── SIGTERM (if needed) ────────►  │
```

---

## Methods

### `initialize`

Handshake. Must be the first request sent after spawning the plugin.

**Request params:**

| Field | Type | Description |
|---|---|---|
| `protocol_version` | string | Protocol version ail speaks (currently `"1"`) |
| `ail_version` | string | ail version string (informational) |

**Response result:**

| Field | Type | Description |
|---|---|---|
| `name` | string | Runner's declared name (e.g. `"codex"`) |
| `version` | string | Runner extension version |
| `protocol_version` | string | Protocol version the runner speaks |
| `capabilities` | object | Declared capabilities (see below) |

**Capabilities object:**

| Field | Type | Default | Description |
|---|---|---|---|
| `streaming` | bool | `false` | Runner emits streaming notifications during invoke |
| `session_resume` | bool | `false` | Runner supports resuming sessions by ID |
| `tool_events` | bool | `false` | Runner emits `stream/tool_use` and `stream/tool_result` |
| `permission_requests` | bool | `false` | Runner sends `stream/permission_request` and awaits `permission/respond` |

### `invoke`

Send a prompt and receive a response. If the runner supports streaming, it emits notifications before the final response.

**Request params:**

| Field | Type | Required | Description |
|---|---|---|---|
| `prompt` | string | yes | The prompt text |
| `session_id` | string | no | Session ID to resume (if `session_resume` capability) |
| `model` | string | no | Model override for this invocation |
| `system_prompt` | string | no | System prompt override |
| `tool_policy` | object | no | Tool permission policy (see below) |

**Tool policy object:**

```json
{ "type": "no_tools" }
{ "type": "allowlist", "tools": ["Read", "Grep"] }
{ "type": "denylist", "tools": ["Bash"] }
{ "type": "mixed", "allow": ["Read"], "deny": ["Bash"] }
```

Omitted or `null` means runner default.

**Response result:**

| Field | Type | Description |
|---|---|---|
| `response` | string | The complete response text |
| `cost_usd` | number? | Cost in USD (null if unavailable) |
| `session_id` | string? | Session ID for future resumption |
| `input_tokens` | number | Input token count (0 if unavailable) |
| `output_tokens` | number | Output token count (0 if unavailable) |
| `thinking` | string? | Concatenated thinking/reasoning text |
| `model` | string? | Model name actually used |
| `tool_events` | array | Ordered tool call/result events (empty if none) |

### `permission/respond`

Sent by ail in response to a `stream/permission_request` notification from the runner.

**Request params:**

| Field | Type | Description |
|---|---|---|
| `allow` | bool | Whether the tool call is allowed |
| `reason` | string? | Reason for denial (when `allow` is false) |

**Response:** Empty result `{}` on success.

### `shutdown`

Request graceful shutdown. The plugin should release resources and prepare to exit.

**Request params:** None.

**Response:** Empty result `{}`.

After receiving the shutdown response, ail closes stdin. If the plugin does not exit within 5 seconds, ail sends SIGTERM. If still running after another 5 seconds, ail sends SIGKILL.

---

## Notifications (plugin → ail)

All notifications are optional. A minimal plugin can skip all notifications and return only the invoke response.

### `stream/delta`

Incremental text output.

```json
{"jsonrpc":"2.0","method":"stream/delta","params":{"text":"Hello"}}
```

### `stream/thinking`

Reasoning/thinking text (extended thinking).

```json
{"jsonrpc":"2.0","method":"stream/thinking","params":{"text":"Let me think..."}}
```

### `stream/tool_use`

A tool call was initiated.

```json
{"jsonrpc":"2.0","method":"stream/tool_use","params":{"tool_name":"Bash","tool_use_id":"toolu_123","input":{"command":"ls"}}}
```

### `stream/tool_result`

A tool call completed.

```json
{"jsonrpc":"2.0","method":"stream/tool_result","params":{"tool_name":"Bash","tool_use_id":"toolu_123","content":"file1.txt\nfile2.txt","is_error":false}}
```

### `stream/cost_update`

Updated token counts and cost.

```json
{"jsonrpc":"2.0","method":"stream/cost_update","params":{"cost_usd":0.05,"input_tokens":1000,"output_tokens":500}}
```

### `stream/permission_request`

The runner needs a tool permission decision before proceeding. ail must respond with a `permission/respond` request.

```json
{"jsonrpc":"2.0","method":"stream/permission_request","params":{"display_name":"Bash","display_detail":"rm -rf /tmp/test","tool_input":{"command":"rm -rf /tmp/test"}}}
```

The plugin **must block** until it receives the `permission/respond` request before continuing.

---

## Error Codes

Standard JSON-RPC 2.0 error codes:

| Code | Meaning |
|---|---|
| `-32700` | Parse error — invalid JSON |
| `-32600` | Invalid request — not a valid JSON-RPC request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32603` | Internal error |

Application-level errors (reserved range `-32000` to `-32099`):

| Code | Meaning |
|---|---|
| `-32001` | Model not available |
| `-32002` | Authentication failed |
| `-32003` | Rate limited |
| `-32004` | Context length exceeded |

---

## Cancellation

When the user cancels an in-flight invocation (e.g. Ctrl+C), ail sends SIGTERM to the plugin process. The plugin should:

1. Stop generating output
2. Return a partial response if possible
3. Exit cleanly

If the plugin does not exit within 5 seconds of SIGTERM, ail sends SIGKILL.

---

## Minimum Viable Plugin

A minimal plugin that echoes prompts:

```python
#!/usr/bin/env python3
import json, sys

def respond(id, result):
    print(json.dumps({"jsonrpc": "2.0", "id": id, "result": result}), flush=True)

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    id = msg.get("id")

    if method == "initialize":
        respond(id, {
            "name": "echo",
            "version": "0.1.0",
            "protocol_version": "1",
            "capabilities": {"streaming": False, "session_resume": False,
                             "tool_events": False, "permission_requests": False}
        })
    elif method == "invoke":
        prompt = msg["params"]["prompt"]
        respond(id, {
            "response": f"Echo: {prompt}",
            "input_tokens": len(prompt.split()),
            "output_tokens": len(prompt.split()) + 1,
        })
    elif method == "shutdown":
        respond(id, {})
        break
```

---

## Versioning

The `protocol_version` field enables forward compatibility. ail and the plugin exchange versions during `initialize`. If versions are incompatible, the plugin should return an error response.

Protocol version `"1"` is the initial version defined in this document.
