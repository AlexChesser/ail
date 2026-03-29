## 13. Human-in-the-Loop (HITL) Gates

HITL gates are intentional checkpoints, not error states.

### 13.1 Explicit HITL Step

```yaml
- id: approve_before_deploy
  action: pause_for_human
  message: "Pipeline complete. Approve to continue."
  timeout_seconds: 3600
  on_timeout: abort_pipeline
```

### 13.2 HITL Responses

| Response | Effect |
|---|---|
| **Approve** | Gate clears. Pipeline continues unchanged. |
| **Reject** | Pipeline aborts. Reason logged to pipeline run log. |
| **Modify** | Human edits the step output or tool input before execution resumes. |
| **Allow for session** | Tool is added to the in-memory session allowlist. Subsequent identical tool calls in this session are auto-approved silently. |

### 13.3 Tool Permission HITL

When a pipeline step invokes a tool not covered by `tools.allow` or `tools.deny` (see §5.8), `ail` intercepts the permission callback via an MCP bridge subprocess and presents it to the human before the tool executes.

**Implementation (v0.1 — validated):** `ail` uses Claude CLI's `--permission-prompt-tool <mcp_tool_name>` flag, not `--permission-prompt-tool stdio`. The actual mechanism is:

1. Before spawning Claude CLI, `ail` writes a temporary MCP config file registering `ail mcp-bridge --socket <path>` as an MCP server.
2. Claude CLI spawns `ail mcp-bridge` as a subprocess and communicates with it via JSON-RPC 2.0 over stdio.
3. For each permission request, the MCP bridge connects to the Unix domain socket, sends a JSON line with `{tool_name, tool_input}`, and blocks until `ail`'s listener thread replies.
4. The main `ail` process runs a Unix socket listener thread that forwards requests to the TUI and blocks until the user responds.
5. The response flows back: listener thread → socket → MCP bridge → MCP response to Claude CLI.

The MCP tool result text contains one of:

```json
// Allow this tool call once
{ "behavior": "allow" }

// Deny this tool call
{ "behavior": "deny", "message": "User rejected this action" }
```

**`updatedInput`** support (Modify response) is deferred to v0.2. It requires `{ "behavior": "allow", "updatedInput": { ...corrected parameters... } }` in the MCP tool result.

**Allow for session** is managed entirely in `ail`'s session state, not in the Claude CLI. When the user selects this option, `ail` records the tool name in an in-memory allowlist. For the remainder of the session, matching permission requests receive an automatic `{"behavior": "allow"}` without prompting.

**Headless mode:** When `--headless` is passed, the MCP bridge is not configured and `--dangerously-skip-permissions` is used instead. No permission prompts occur.

**Custom providers (Ollama, Bedrock, etc.):** When a pipeline step overrides `provider.base_url`, the MCP bridge is NOT configured for that step. Third-party providers do not implement Claude's permission model, and small local models will spuriously call any MCP tool visible in their context, causing the pipeline to block indefinitely. Permission HITL applies only to steps that use the default Claude API endpoint.

#### Permission Mode

`ail` uses Claude CLI's `--permission-prompt-tool` flag to delegate all permission decisions to the MCP bridge. The `--permission-mode` flag is not used in v0.1; a future version may expose it as a session-level option.

### 13.4 Tool Permission Flow

```
Claude CLI wants to invoke a tool
  ↓
Is tool in step's tools.allow? (--allowedTools)
  YES → tool executes — no MCP bridge involved
  ↓ NO
Is tool in step's tools.deny? (--disallowedTools)
  YES → tool denied — no MCP bridge involved
  ↓ NO
Claude CLI calls ail_check_permission MCP tool
  ↓
ail mcp-bridge receives tools/call, forwards to Unix socket
  ↓
ail listener thread receives request, checks session allowlist
  → In allowlist? YES → { "behavior": "allow" } — silent, no prompt
  ↓ NO
TUI shows permission modal (tool name + truncated input JSON)
  → y (Approve once)       → { "behavior": "allow" }
  → a (Allow for session)  → { "behavior": "allow" } + add tool to session allowlist
  → n (Deny)               → { "behavior": "deny", "message": "User rejected" }
  [Modify deferred to v0.2]
  ↓
Response written to Unix socket → MCP bridge → MCP tool result → Claude CLI
```

**v0.1 supported responses:** approve once, allow for session, deny.
**v0.2 deferred:** modify (edit tool input before allowing — requires `updatedInput` in MCP response).

### 13.5 Implicit HITL via `on_result`

Preferred over explicit gates — interrupts only when something genuinely requires attention. See §5.4.

### 13.6 Headless / Automated Mode

For automated runs (CI, the autonomous agent use case, Docker sandbox), HITL prompts are not viable. Pass `--dangerously-skip-permissions` to the Claude CLI invocation to bypass all tool permission checks. This is only appropriate in a fully trusted, sandboxed environment. `ail` will expose this as a session-level flag — not a pipeline YAML option — to prevent it from being accidentally committed to a shared pipeline file.

---
