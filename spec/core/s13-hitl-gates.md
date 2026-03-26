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

When a pipeline step invokes a tool not covered by `tools.allow` or `tools.deny` (see §5.8), `ail` intercepts the permission callback via `--permission-prompt-tool stdio` and presents it to the human before the tool executes.

`ail` reads permission request events from the NDJSON stream, renders them in the TUI, and writes a JSON response back to Claude CLI's stdin. The full set of valid responses is:

```json
// Allow this tool call once
{ "behavior": "allow" }

// Allow with modified tool input (the Modify response)
{ "behavior": "allow", "updatedInput": { ...corrected parameters... } }

// Deny this tool call
{ "behavior": "deny", "message": "User rejected this action" }
```

**`updatedInput`** is how the **Modify** HITL response is implemented. The human edits the tool's input parameters in `ail`'s TUI — correcting a file path, adjusting a command, removing a sensitive value — before allowing execution to proceed with the corrected values.

**Allow for session** is managed entirely in `ail`'s session state, not in the Claude CLI. When the user selects this option, `ail` records the tool name and input pattern in an in-memory allowlist. For the remainder of the session, matching permission requests receive an automatic `{"behavior": "allow"}` without prompting.

#### Permission Mode

`ail` launches Claude CLI in `default` permission mode unless configured otherwise. The supported modes — `default`, `accept_edits`, `plan`, `bypass_permissions` — map to `--permission-mode` flag values. This may be exposed as a session-level option in a future version. For headless runs, `--dangerously-skip-permissions` (or `bypass_permissions` mode) is the correct approach.

> **Implementation note:** The `--permission-prompt-tool stdio` behaviour when combined with `-p` (non-interactive prompt mode) must be validated in the v0.0.1 spike. The VSCode extension uses this mechanism in interactive mode; `ail`'s non-interactive usage pattern may differ. See `RUNNER-SPEC.md`.

### 13.4 Tool Permission Flow

```
Claude CLI emits tool_use event
  ↓
Is tool in step's tools.allow?
  YES → { "behavior": "allow" } — silent, no prompt
  ↓ NO
Is tool in step's tools.deny?
  YES → { "behavior": "deny" } — silent, no prompt
  ↓ NO
Is tool in session allowlist?
  YES → { "behavior": "allow" } — silent, no prompt
  ↓ NO
Present HITL prompt to human
  → Approve      → { "behavior": "allow" }
  → Allow for session → { "behavior": "allow" } + add to session allowlist
  → Modify       → { "behavior": "allow", "updatedInput": <edited> }
  → Reject       → { "behavior": "deny", "message": "User rejected" }
```

### 13.5 Implicit HITL via `on_result`

Preferred over explicit gates — interrupts only when something genuinely requires attention. See §5.4.

### 13.6 Headless / Automated Mode

For automated runs (CI, the autonomous agent use case, Docker sandbox), HITL prompts are not viable. Pass `--dangerously-skip-permissions` to the Claude CLI invocation to bypass all tool permission checks. This is only appropriate in a fully trusted, sandboxed environment. `ail` will expose this as a session-level flag — not a pipeline YAML option — to prevent it from being accidentally committed to a shared pipeline file.

---
