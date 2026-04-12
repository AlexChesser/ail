## 13. Human-in-the-Loop (HITL) Gates

> **Implementation status:** `pause_for_human` action is implemented in `execute_with_control()` (the controlled executor used by TUI and `--output-format json` mode): it blocks execution and emits a `HitlGateReached` event. In simple `execute()` mode (`--once` text output), `pause_for_human` is a no-op. `modify_output` action (§13.2) is fully implemented: in controlled mode it emits `HitlModifyReached`, blocks for human input, and stores the modified text; in headless mode behavior is configurable via `on_headless` (skip/abort/use_default). "Allow for session" is implemented in both `--once --output-format json` and `ail chat` (NDJSON) modes via the `allow_for_session` field in the `permission_response` stdin message (§23.7). The allowlist persists for the lifetime of the run/chat session — in chat mode it carries across turns. The "Modify" response for tool permissions is deferred to v0.3.

HITL gates are intentional checkpoints, not error states.

### 13.1 Explicit HITL Steps

#### `pause_for_human`

```yaml
- id: approve_before_deploy
  action: pause_for_human
  message: "Pipeline complete. Approve to continue."
  timeout_seconds: 3600
  on_timeout: abort_pipeline
```

#### `modify_output`

Presents the most recent step output to the human for editing. The human-modified text is stored in the turn log and available to subsequent steps via `{{ step.<id>.modified }}`.

```yaml
- id: generate
  prompt: "Generate a document"
- id: review_gate
  action: modify_output
  message: "Review and edit the generated document before continuing."
  on_headless: skip          # skip | abort | use_default (default: skip)
  # default_value: "..."     # required when on_headless: use_default
- id: finalize
  prompt: "Finalize: {{ step.review_gate.modified }}"
```

**`on_headless`** controls behavior in headless/`--once` text mode:

| Value | Effect |
|---|---|
| `skip` (default) | Gate is skipped; no entry recorded; pipeline continues. |
| `abort` | Pipeline aborts with `PIPELINE_ABORTED`. |
| `use_default` | Uses `default_value` as the modified output; entry recorded. |

When `on_headless: use_default` is specified, `default_value` is required — validation rejects the pipeline if it is missing.

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

Runners that do not support tool permissions ignore the `permission_responder` field. Runners in headless mode bypass permission HITL entirely. Custom providers (Ollama, Bedrock, etc.) support interactive permission HITL via the MCP bridge — `--permission-prompt-tool` is internal to Claude CLI and works regardless of backend.

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

**`--once --output-format json` mode and `ail chat` (NDJSON) mode:** Full interactive HITL is available. Both the invocation step and all pipeline steps use a `permission_responder` wired to the stdin control protocol (§23.7). Consumers receive `permission_requested` events on stdout and respond via `permission_response` messages on stdin. `pause_for_human` steps and `on_result: pause_for_human` branches both block the executor and emit `hitl_gate_reached` events, unblocked by `hitl_response` messages on stdin. `modify_output` steps emit `hitl_modify_reached` events (which include the `last_response` for the human to edit) and are similarly unblocked by `hitl_response` messages — the response text becomes the modified output. "Allow for session" (`allow_for_session: true`) adds the tool's `display_name` to an in-memory allowlist shared across turns in chat mode; subsequent permission requests for the same tool are auto-approved silently and no `permission_requested` event is emitted for them.

**`--once` text mode:** The `--once --output-format text` flow does not set a `permission_responder` and does not spawn a stdin reader thread. Interactive permission HITL is not available. Tools in text mode require either `--headless` (bypass all permissions) or `tools: allow:` in the pipeline YAML (pre-approve specific tools).

When `pause_for_human` fires in headless / text mode (either as an explicit step or via `on_result: pause_for_human`), the executor emits a `WARN`-level log message identifying the step and any configured message, then **continues the pipeline**. No HITL gate is raised and no input is awaited. This is visible on stderr (as a structured JSON log entry) when the binary's tracing subscriber is active. Use `--output-format json` mode for interactive HITL gates.

When `modify_output` fires in headless / text mode, the behavior depends on the step's `on_headless` field:
- **`skip`** (default): the gate is skipped with a `WARN`-level log message; no turn entry is recorded; the pipeline continues. The `{{ step.<id>.modified }}` variable will not be available — subsequent steps that reference it will fail with `TEMPLATE_UNRESOLVED`.
- **`abort`**: the pipeline terminates with a `PIPELINE_ABORTED` error.
- **`use_default`**: the `default_value` is used as the modified output; a turn entry is recorded; `{{ step.<id>.modified }}` resolves to the default value.

**Interactive questions in Claude's text output:** When the model's response contains a question (e.g., "Do you want option 1, 2, or 3?"), this is the step's completed response — not a HITL event. In `-p` mode (single-turn), there is no follow-up turn within a step. The question text is available to subsequent steps as `{{ step.<id>.response }}`. Pipelines that need human review of model output should use explicit `pause_for_human` gates.

---
