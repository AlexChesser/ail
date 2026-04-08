## Reference Implementation — Claude CLI

The Claude CLI (`claude`) is the reference implementation for this specification. It is the only first-class runner in v0.0.1. All contract decisions are validated against Claude CLI behaviour first.

### Invocation Model

The Claude CLI supports a structured bidirectional JSON interface that `ail` uses instead of PTY wrapping:

| Direction | Flag | Format |
|---|---|---|
| Output from Claude | `--output-format stream-json` | NDJSON event stream on stdout |
| Input to Claude | `--input-format stream-json` | NDJSON messages on stdin |
| Prompt (non-interactive) | `-p "<prompt>"` or `--print "<prompt>"` | Plain string |

### Output Event Stream

`--output-format stream-json` produces a newline-delimited stream of JSON events. Key event types:

```json
// Session initialised
{ "type": "system", "subtype": "init", "session_id": "abc123", ... }

// Assistant tool call
{ "type": "assistant", "message": {
    "content": [{ "type": "tool_use", "id": "toolu_abc", "name": "Write",
                  "input": { "file_path": "./foo.txt", "content": "..." } }]
}}

// Tool result fed back
{ "type": "user", "message": {
    "content": [{ "type": "tool_result", "tool_use_id": "toolu_abc", ... }]
}}

// Run complete
{ "type": "result", "subtype": "success",
  "result": "<final text response>",
  "total_cost_usd": 0.003,
  "session_id": "abc123" }

// Run failed
{ "type": "result", "subtype": "error", "error": "...", "session_id": "abc123" }
```

**Completion signal:** `ail` considers a Claude CLI invocation complete when it receives a `result` event. `subtype: success` → pipeline step succeeded. `subtype: error` → `on_error` handling fires.

**Cost tracking:** `total_cost_usd` in the result event feeds directly into `ail/budget-gate` without any external token counting.

### Tool Permission Interface

**Spike validated — v0.1 implemented.** `--permission-prompt-tool stdio` does NOT work with `-p` mode. The correct mechanism is `--permission-prompt-tool <mcp_tool_name>` where the tool name refers to an MCP tool exposed by a subprocess registered in a `--mcp-config` JSON file.

When `ail` needs to intercept tool permissions (for tools not covered by `tools.allow` or `tools.deny`), it launches Claude CLI with:

```
--mcp-config <tmp_config.json> --permission-prompt-tool mcp__ail-permission__ail_check_permission
```

The temporary MCP config registers `ail mcp-bridge --socket <path>` as an MCP server. Claude CLI spawns this subprocess and calls `ail_check_permission` (a tool exposed by the bridge) for each permission decision.

The MCP bridge forwards requests to the main `ail` process via a Unix domain socket. The response is one of:

```json
{ "behavior": "allow" }

{ "behavior": "deny", "message": "User rejected" }
```

`{ "behavior": "allow", "updatedInput": { ...modified tool input... } }` is valid per the protocol but deferred to v0.2 (requires inline editor in TUI).

**IPC topology:**
```
Claude CLI ──[MCP stdio]──► ail mcp-bridge ──[Unix socket]──► ail listener thread
                                                                      ↕ PermissionResponder callback
                                                                  TUI permission modal
```

**Lifecycle:** For each `invoke_streaming` call with a `permission_responder` set in `InvokeOptions`, `ClaudeCliRunner`:
1. Creates a temporary Unix socket path (`/tmp/ail-perm-<uuid>.sock`)
2. Spawns a listener thread that binds the socket and signals readiness
3. Waits for the ready signal before proceeding (avoids a race with MCP bridge connect)
4. Writes a temporary MCP config file (`/tmp/ail-mcp-config-<uuid>.json`)
5. Passes both paths to Claude CLI via `--mcp-config` and `--permission-prompt-tool`
6. Cleans up both temp files after Claude CLI exits

The socket lifecycle is entirely encapsulated in `ClaudeCliRunner::invoke_streaming`. The caller (TUI or executor) only provides a `PermissionResponder` callback — it never handles socket paths or raw JSON.

**Headless mode:** The MCP config and `--permission-prompt-tool` are omitted when `--headless` is active; `--dangerously-skip-permissions` is used instead.

**Custom providers:** The MCP bridge works with custom providers (Ollama, Bedrock, etc.) because `--permission-prompt-tool` is internal to Claude CLI — the model never sees the bridge tool in its tool list. Claude CLI intercepts tool calls before execution regardless of the backend API.

#### Permission Modes

`ail` does not pass `--permission-mode` to Claude CLI in v0.1. Claude CLI's default permission mode applies, and unresolved tool requests are delegated to `ail_check_permission` via the MCP bridge.

Claude CLI supports `--permission-mode` values including `default`, `accept_edits`, `plan`, `bypass_permissions`, `delegate`, and `dont_ask`. Exposing these as session-level options is deferred to v0.2.

#### AskUserQuestion Intercept

##### Why a custom MCP tool is used

Claude CLI's native `AskUserQuestion` tool enforces a strict input schema at validation time — before the `--permission-prompt-tool` intercept fires. In practice, different models (and even the same model inconsistently) produce inputs that fail this validation: missing `description` fields on options, `questions` sent as a JSON-encoded string rather than an array, and so on. Once validation fails, the model receives an error and typically degrades (retrying with progressively worse payloads) rather than recovering gracefully.

To avoid this, `ail` exposes its own lenient `ail_ask_user` tool through the MCP bridge and disallows the native `AskUserQuestion` when the bridge is active. The bridge normalises whatever the model sends before forwarding it to the permission socket as an `AskUserQuestion` event — so all downstream code (the permission listener, the VS Code frontend) remains unchanged.

##### `ail_ask_user` tool schema

The bridge's `tools/list` response includes a second tool alongside `ail_check_permission`:

```json
{
  "name": "ail_ask_user",
  "description": "Ask the user a question with optional multiple-choice options. Use this instead of AskUserQuestion.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "question":     { "type": "string", "description": "The question to ask." },
      "header":       { "description": "Optional title." },
      "multiSelect":  { "description": "Boolean or string 'true'/'false'." },
      "options":      { "description": "Array of strings or {label, description?} objects, or a JSON string." },
      "questions":    { "description": "Alternative: array of {header, question, multiSelect, options} objects." }
    },
    "required": []
  }
}
```

Only `question` is semantically required; all other fields are optional and type-coerced during normalisation.

##### Normalisation performed by the bridge

`normalize_ask_user_input` in `ail/src/mcp_bridge.rs` converts any model-produced format into the canonical shape before forwarding:

| Input variation | Normalisation |
|---|---|
| `questions` as JSON-encoded string | Parsed to array |
| Flat `{question, options}` (no `questions` wrapper) | Wrapped as `{questions: [{...}]}` |
| `options` as JSON-encoded string | Parsed to array |
| `options` elements as bare strings | Converted to `{label: string}` |
| Missing `description` on option object | Omitted (not defaulted) |
| `multiSelect` as string `"true"`/`"false"` | Coerced to boolean |
| Missing `header` | Defaulted to `""` |

The normalised payload is forwarded to the permission socket with `tool_name: "AskUserQuestion"`, so the permission listener and VS Code frontend require no changes.

##### Response flow

When a `PermissionRequest` arrives with `display_name == "AskUserQuestion"`, the VS Code extension (and any other UI consumer) intercepts it as a structured question rather than a generic permission prompt. The `tool_input` field on `PermissionRequest` carries the normalised JSON tool input:

```json
{
  "questions": [
    {
      "header": "Clarification needed",
      "question": "Which framework should I use?",
      "multiSelect": false,
      "options": [
        { "label": "React", "description": "Component-based UI library" },
        { "label": "Vue" }
      ]
    }
  ]
}
```

The UI renders this as a radio/checkbox question card. When the user answers, the response is sent as a **deny** with the answer text as the reason:

```json
{ "behavior": "deny", "message": "<user's answer>" }
```

The MCP bridge extracts the `message` field and returns it as the `ail_ask_user` tool result text. The model receives the answer as a clean tool output and can proceed accordingly.

##### Native `AskUserQuestion` is disallowed when the bridge is active

`ClaudeCliRunner` adds `--disallowedTools AskUserQuestion` and an `--append-system-prompt` instruction to Claude CLI's arguments whenever the MCP bridge is configured (non-headless mode). This ensures the model uses `mcp__ail-permission__ail_ask_user` exclusively and never encounters Claude CLI's strict validation.

The `tool_input` field was added to `PermissionRequest` (as `Option<serde_json::Value>`, serialised with `skip_serializing_if = "Option::is_none"`) specifically to support this pattern. `ClaudeCliRunner` populates it from the MCP bridge request; other runners may leave it as `None`.

#### `PreToolUse` Hook (Alternative Intercept)

As an alternative to `--permission-prompt-tool`, Claude CLI supports a `PreToolUse` hook — a process that runs synchronously after Claude creates tool parameters but before the tool executes. This is more suitable for automated validation than for interactive HITL; `ail`'s primary HITL mechanism is the MCP bridge.

### Headless Mode

When `ail` is invoked with `--headless` (required for automated runs such as CI and the SWE-bench benchmarking experiment), it passes `--dangerously-skip-permissions` to the Claude CLI. This bypasses all tool permission checks — no HITL prompts, no `--permission-prompt-tool` intercept.

`ail --headless` maps to:

```
claude --output-format stream-json --verbose --dangerously-skip-permissions -p <prompt>
```

`pause_for_human` actions in headless mode abort the pipeline immediately (default) or auto-approve (if `--headless-approve` is set). This is a session-level flag, never a pipeline YAML option — it must not be committable to a shared pipeline file.

> **Security:** Only use headless mode in a sandboxed or fully trusted environment. `--dangerously-skip-permissions` grants the model unrestricted tool access.

### Tool Policy

`tools.disabled`, `tools.allow`, and `tools.deny` in the pipeline step map to Claude CLI flags as:

| YAML field | Claude CLI flag | Effect |
|---|---|---|
| `disabled: true` | `--tools ""` | Removes all tool definitions from the model's context entirely |
| `allow: [...]` | `--allowedTools <list>` | Pre-approve these tools; executes silently |
| `deny: [...]` | `--disallowedTools <list>` | Pre-deny these tools; rejected silently |

`disabled` takes priority — if `disabled: true` is set, `allow` and `deny` are ignored.

Pattern syntax (e.g. `Bash(git log*)`, `Edit(./src/*)`) is passed verbatim — `ail` does not parse or validate patterns.

> **Note:** `--system-prompt` and `--append-system-prompt` **append to** Claude CLI's base system prompt — they do not replace it. Even with `--bare`, Claude CLI injects its own session context (date, environment info, etc.) before user-provided system prompt additions. For classification steps or other steps where the model must follow specific instructions reliably, put those instructions in the `prompt:` field (the user message), not only in `system_prompt:`. This is especially important for small models where the combined system prompt is too large to process reliably.

### Context and Session Continuity

**Resolved in v0.0.1 spike.** The `session_id` returned in each `result` event can be passed back as `--resume <session_id>` to resume the conversation in a subsequent subprocess invocation. `ail` uses one process per pipeline step, resuming via `--resume` between steps. `--input-format stream-json` is not used.

The `--resume` flag is not documented in the Claude CLI `--help` output but is functional. It preserves full conversation history across invocations, enabling pipeline steps to reference prior outputs naturally (e.g. "Review the above output.") without template variable injection.

`ail`'s session chaining model:
1. `invocation` step runs; its `result` event carries `session_id` → stored in turn log
2. Each subsequent pipeline step spawns a new subprocess with `--resume <last_session_id>` and `-p <resolved_prompt>`
3. The runner has full conversation history; template variable injection is used for cross-step references outside the active conversation thread

### Flags Summary

| Flag | Purpose | `ail` usage |
|---|---|---|
| `--output-format stream-json` | Structured NDJSON event stream | Always |
| `--input-format stream-json` | Accept NDJSON messages on stdin | When session continuation needed |
| `-p / --print` | Non-interactive prompt | Single-turn steps |
| `--mcp-config <path>` | Register MCP bridge for permission intercept | Written per-run to `/tmp/ail-mcp-config-<uuid>.json` |
| `--permission-prompt-tool mcp__ail-permission__ail_check_permission` | Delegate permission decisions to MCP tool | When non-headless and step has unspecified tools |
| `--tools ""` | Disable all tools | From `tools.disabled: true` |
| `--allowedTools` | Pre-approve tools | From `tools.allow` |
| `--disallowedTools` | Pre-deny tools | From `tools.deny` |
| `--permission-mode` | Set permission enforcement level | Session-level; `default` unless overridden |
| `--dangerously-skip-permissions` | Bypass all permission checks | Headless/automated mode only |
| `--verbose` | Required with `--output-format stream-json -p` | Always — omitting it causes an error |
| `--resume <session_id>` | Resume a prior session by ID | Between pipeline steps |
| `--model <name>` | Override the model for this invocation | From `defaults.model`, per-step `model:`, or `--model` CLI flag |
| `--verbose --include-partial-messages` | Token-level streaming | Observability / debugging |

**Note:** `--output-format stream-json` requires `--verbose` when used with `-p` (non-interactive mode). Omitting `--verbose` produces the error: _"When using --print, --output-format=stream-json requires --verbose"_. This is undocumented in Claude CLI's `--help` output.

**Note:** Claude CLI sets the `CLAUDECODE` environment variable. When `ail` spawns a subprocess, it must remove this variable from the child environment — Claude CLI blocks nested sessions if it detects it is running inside another Claude Code session.

**Note:** The Claude CLI respects `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN` environment variables, allowing `ail` to redirect API calls to alternative providers (e.g. local Ollama at `http://localhost:11434`). These are set **per subprocess** via `Command::env()` — they are never exported to the parent process environment. Set via pipeline `defaults.provider.base_url`/`defaults.provider.auth_token` in the YAML, or overridden via `--provider-url`/`--provider-token` CLI flags.

*Spike validation status: resolved for Claude CLI v0.0.1. Flags above reflect verified behaviour. Permission HITL mechanism (MCP bridge) validated and implemented in v0.1.*

---
