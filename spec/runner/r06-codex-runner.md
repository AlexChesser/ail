## Codex Runner

The Codex runner wraps the OpenAI Codex CLI (`@openai/codex`) in its non-interactive
`codex exec --json` mode. It targets OpenAI models and is the second first-class runner
in `ail` after the Claude CLI runner.

---

### 1. Purpose

`CodexRunner` adapts the Codex CLI's item-lifecycle NDJSON stream to `ail`'s `Runner`
trait. It provides streaming output, session continuity (via thread IDs), extended thinking
(via reasoning items), and tool event capture — without requiring any changes to the executor
or the pipeline language.

---

### 2. Invocation

**CLI form:**
```
codex exec [resume <thread_id>] --json [--model <model>] [--full-auto] <prompt>
```

**Environment variables:**

| Variable | Purpose | Default |
|---|---|---|
| `OPENAI_API_KEY` | Authentication — read by the `codex` binary | *(must be set by operator)* |
| `AIL_CODEX_BIN` | Path or name of the `codex` executable | `"codex"` |

`OPENAI_API_KEY` is **not** set or modified by `ail`. It must be present in the ambient
environment before `ail` is invoked.

`AIL_CODEX_BIN` lets operators pin a specific version or path (`/usr/local/bin/codex-1.2.3`)
without modifying `PATH`.

**Install the Codex CLI:**
```bash
npm i -g @openai/codex
```

---

### 3. Session Resumption

When `InvokeOptions::resume_session_id` is set, the runner passes it as:
```
codex exec resume <thread_id> --json ...
```

The `thread_id` is obtained from the `thread.started` event in the previous invocation's
stream and is stored as `RunResult::session_id`. It is the Codex equivalent of Claude's
`--resume <session_id>`.

---

### 4. Wire Format

`codex exec --json` produces a newline-delimited stream of JSON events (NDJSON) on stdout.
The stream uses an **item lifecycle model**: each logical work unit (agent message, command
execution, reasoning) progresses through `item.started` → `item.updated` → `item.completed`
events.

**Key event types:**

```json
// Session initialised
{ "type": "thread.started", "thread_id": "thread_abc123" }

// Text streaming (agent_message item in progress)
{ "type": "item.updated", "item_type": "agent_message",
  "item": { "text": "Here is my answer...", "status": "in_progress" } }

// Shell command started
{ "type": "item.started", "item_type": "command_execution",
  "item_id": "cmd_xyz", "item": { "command": "ls -la", "status": "in_progress" } }

// Shell command completed
{ "type": "item.completed", "item_type": "command_execution",
  "item_id": "cmd_xyz",
  "item": { "aggregated_output": "total 8\ndrwxr-xr-x ...", "exit_code": 0,
            "status": "completed" } }

// Reasoning block in progress (extended thinking)
{ "type": "item.updated", "item_type": "reasoning",
  "item": { "text": "Let me consider...", "status": "in_progress" } }

// Final agent message (response text)
{ "type": "item.completed", "item_type": "agent_message",
  "item": { "text": "The answer is 42.", "status": "completed" } }

// Turn finished successfully
{ "type": "turn.completed" }

// Turn failed
{ "type": "turn.failed", "error": "rate limit exceeded" }

// Protocol-level error
{ "type": "error", "message": "connection refused" }
```

**Completion signal:** `ail` considers a Codex invocation complete when it receives a
`turn.completed` event. `turn.failed` or `error` → `on_error` handling fires.

**Token counts:** The `codex exec --json` wire format does not expose per-turn token usage.
`RunResult::input_tokens` and `RunResult::output_tokens` are always `0` for this runner.
`RunResult::cost_usd` is always `None`.

---

### 5. Decoder Event Map

`CodexNdjsonDecoder` processes the following events:

| Wire `event_type` | `item_type` | Action |
|---|---|---|
| `thread.started` | — | Store `thread_id` → becomes `RunResult::session_id` |
| `item.updated` | `agent_message` | Emit `StreamDelta { text }` |
| `item.updated` | `reasoning` | Accumulate thinking; emit `Thinking { text }` |
| `item.started` | `command_execution` | Emit `ToolUse { tool_name: "Bash", tool_use_id: item_id, input: {command} }`; push `tool_call` to `tool_events` |
| `item.completed` | `command_execution` | Emit `ToolResult`; push `tool_result` to `tool_events` |
| `item.completed` | `agent_message` | Overwrite `response` with final `item.text` |
| `turn.completed` | — | Set `done = true` |
| `turn.failed` | — | Set `error = event.error`, `done = true` |
| `error` | — | Set `error = event.message`, `done = true` |
| Anything else | — | `tracing::trace!`, continue |

When multiple `item.completed` / `agent_message` events arrive in a single turn (unlikely
but possible), the **last one wins** — its text becomes the final response.

---

### 6. Unsupported `InvokeOptions` Fields

The Codex CLI does not expose the same hook and tool-policy mechanisms as the Claude CLI.
The following `InvokeOptions` fields are ignored:

| Field | Reason | Log level |
|---|---|---|
| `tool_policy` (non-default) | Codex uses sandbox levels, not named tool allowlists | `WARN` |
| `system_prompt` | No CLI equivalent | `TRACE` |
| `append_system_prompt` | No CLI equivalent | `TRACE` |
| `permission_responder` | No hook mechanism in the Codex CLI | `TRACE` |

`tool_policy: RunnerDefault` is silent (no log).

Pipeline authors who need tool control with Codex should rely on Codex's native sandbox
levels (controlled via `--full-auto` / headless mode) rather than `ail` tool policies.

---

### 7. Headless Mode

When `CodexRunnerConfig::headless` is `true`, the runner adds `--full-auto` to the
invocation:
```
codex exec --json --full-auto <prompt>
```

`--full-auto` enables Codex's automated execution sandbox: tool calls are approved without
per-call prompts. This is the Codex equivalent of Claude's `--dangerously-skip-permissions`
and is required for CI and other automated environments.

When `headless` is `false` (the default), Codex runs in its default interactive sandbox
mode — tool calls may require confirmation depending on the Codex version and configuration.

---

### 8. Factory Registration

`CodexRunner` is registered in `RunnerFactory` under the name `"codex"` (case-insensitive):

```yaml
# .ail.yaml
steps:
  - id: my_step
    runner: codex
    prompt: "Describe the files in this directory."
```

The factory reads `AIL_CODEX_BIN` and the `headless` flag from the pipeline's execution
context. All other Codex configuration is read from the ambient environment by the `codex`
binary itself.
