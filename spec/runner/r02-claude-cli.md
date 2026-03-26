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

When `ail` needs to intercept tool permissions (for tools not covered by `tools.allow` or `tools.deny`), it launches Claude CLI with:

```
--permission-prompt-tool stdio
```

Claude emits a permission request event on the NDJSON stream when it wants to invoke a tool requiring authorisation. `ail` reads the event, presents the HITL UI, and writes one of these responses to the Claude CLI process stdin:

```json
{ "behavior": "allow" }

{ "behavior": "deny", "message": "User rejected" }

{ "behavior": "allow", "updatedInput": { ...modified tool input... } }
```

The `updatedInput` form allows `ail` to present an inline editor — the human corrects a file path, removes a sensitive argument, or adjusts a command — and Claude executes the corrected version rather than its original parameters.

#### Permission Modes

Claude CLI supports six permission modes via `--permission-mode`:

| Mode | Behaviour |
|---|---|
| `default` | Checks `settings.json`, `--allowedTools`, `--disallowedTools`, then calls `--permission-prompt-tool` for anything unresolved |
| `accept_edits` | Auto-accepts file edits; prompts for other tool types |
| `plan` | Read-only; no file modifications or commands |
| `bypass_permissions` | No permission checks at all (equivalent to `--dangerously-skip-permissions`) |
| `delegate` | Delegates permission decisions to the MCP tool specified |
| `dont_ask` | Auto-accepts everything without prompting |

`ail` defaults to `default` mode. For headless/automated runs (Docker sandbox, CI), use `bypass_permissions` or `--dangerously-skip-permissions`. `ail` exposes this as a session-level CLI flag, never as a pipeline YAML option.

#### `PreToolUse` Hook (Alternative Intercept)

As an alternative to `--permission-prompt-tool`, Claude CLI supports a `PreToolUse` hook — a process `ail` registers that runs synchronously after Claude creates tool parameters but before the tool executes. The hook receives `tool_name`, `tool_input`, and `tool_use_id` and can allow, deny, or modify the call.

This is more suitable for automated validation (schema checking, path sanitisation) than for interactive HITL — the hook runs as a subprocess without a human UI. It is noted here for completeness; `ail`'s primary HITL mechanism remains `--permission-prompt-tool stdio`.

> **Spike validation required:** Confirm that `--permission-prompt-tool stdio` behaves correctly when combined with `-p` (non-interactive mode). The VSCode extension uses this combination in interactive mode; `ail`'s usage differs. Document actual permission event shapes from the NDJSON stream.

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
| `--permission-prompt-tool stdio` | HITL tool permission intercept | When step has unspecified tools |
| `--allowedTools` | Pre-approve tools | From `tools.allow` |
| `--disallowedTools` | Pre-deny tools | From `tools.deny` |
| `--permission-mode` | Set permission enforcement level | Session-level; `default` unless overridden |
| `--dangerously-skip-permissions` | Bypass all permission checks | Headless/automated mode only |
| `--verbose` | Required with `--output-format stream-json -p` | Always — omitting it causes an error |
| `--resume <session_id>` | Resume a prior session by ID | Between pipeline steps |
| `--verbose --include-partial-messages` | Token-level streaming | Observability / debugging |

**Note:** `--output-format stream-json` requires `--verbose` when used with `-p` (non-interactive mode). Omitting `--verbose` produces the error: _"When using --print, --output-format=stream-json requires --verbose"_. This is undocumented in Claude CLI's `--help` output.

**Note:** Claude CLI sets the `CLAUDECODE` environment variable. When `ail` spawns a subprocess, it must remove this variable from the child environment — Claude CLI blocks nested sessions if it detects it is running inside another Claude Code session.

*Spike validation status: resolved for Claude CLI v0.0.1. Flags above reflect verified behaviour.*

---
