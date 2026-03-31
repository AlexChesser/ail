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

When a pipeline step encounters a tool not covered by its `tools.allow`/`tools.deny` policy (see §5.8), the executor passes a `PermissionResponder` callback to the runner via `InvokeOptions`. The runner is responsible for:

1. Intercepting the permission decision point in its native protocol.
2. Constructing a `PermissionRequest { display_name, display_detail }` with human-readable fields pre-formatted by the runner.
3. Calling the `PermissionResponder` callback, which blocks until the human decides.
4. Serialising the `PermissionResponse` (Allow / Deny) back to its native protocol.

Runners that do not support tool permissions ignore the `permission_responder` field. Runners in headless mode bypass permission HITL entirely.

**Allow for session** is managed in `ail`'s session state. When the user selects this option, `ail` records the `display_name` in an in-memory allowlist. Subsequent matching permission requests receive an automatic Allow without prompting.

The Claude CLI reference implementation uses an MCP bridge subprocess and Unix domain socket — see `spec/runner/r02-claude-cli.md §Tool Permission Interface`.

### 13.4 Tool Permission Flow

```
Runner wants to invoke a tool
  ↓
Runner checks InvokeOptions.tool_policy
  → Pre-approved (Allowlist)?  YES → tool executes
  → Pre-denied  (Denylist)?    YES → tool denied
  ↓ UNKNOWN
Runner calls PermissionResponder(PermissionRequest { display_name, display_detail })
  ↓
TUI checks session allowlist
  → In allowlist? YES → Allow — silent, no prompt
  ↓ NO
TUI shows permission modal (display_name + display_detail)
  → y (Approve once)       → PermissionResponse::Allow
  → a (Allow for session)  → PermissionResponse::Allow + add display_name to session allowlist
  → n (Deny)               → PermissionResponse::Deny("User rejected")
  [Modify deferred to v0.2]
  ↓
Runner serialises response to its native protocol
```

**v0.1 supported responses:** approve once, allow for session, deny.
**v0.2 deferred:** modify (edit tool input before allowing).

### 13.5 Implicit HITL via `on_result`

Preferred over explicit gates — interrupts only when something genuinely requires attention. See §5.4.

### 13.6 Headless / Automated Mode

For automated runs (CI, the autonomous agent use case, Docker sandbox), HITL prompts are not viable. Pass `--dangerously-skip-permissions` to the Claude CLI invocation to bypass all tool permission checks. This is only appropriate in a fully trusted, sandboxed environment. `ail` will expose this as a session-level flag — not a pipeline YAML option — to prevent it from being accidentally committed to a shared pipeline file.

---
