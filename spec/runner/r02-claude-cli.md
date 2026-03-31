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

#### Permission Modes

`ail` does not pass `--permission-mode` to Claude CLI in v0.1. Claude CLI's default permission mode applies, and unresolved tool requests are delegated to `ail_check_permission` via the MCP bridge.

Claude CLI supports `--permission-mode` values including `default`, `accept_edits`, `plan`, `bypass_permissions`, `delegate`, and `dont_ask`. Exposing these as session-level options is deferred to v0.2.

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

### Pre-Approved Tool Policy

`tools.allow` and `tools.deny` in the pipeline step are passed to Claude CLI as:

```
--allowedTools Read,Edit,Glob
--disallowedTools WebFetch,Bash
```

Pattern syntax (e.g. `Bash(git log*)`, `Edit(./src/*)`) is passed verbatim — `ail` does not parse or validate patterns.

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
