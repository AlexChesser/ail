## r05. HTTP Runner — Direct OpenAI-Compatible API

The HTTP runner (`http`) calls any OpenAI-compatible `/v1/chat/completions` endpoint directly, without wrapping a CLI subprocess. It is the second first-class runner shipped with `ail`, primarily intended for:

- **Classification and transformation steps** where the model must follow precise instructions (system prompt reliability matters)
- **Local models** via Ollama (e.g. `qwen3`, `llama3`) where tool use is not required
- **Any OpenAI-compatible API** (direct Anthropic API, OpenRouter, Groq, etc.)

The `ollama` factory alias is a convenience shorthand for `http` with default settings optimised for a local Ollama instance.

### When to Use the HTTP Runner vs. the Claude CLI Runner

| Concern | HTTP runner | Claude CLI runner |
|---|---|---|
| System prompt control | Full — you own the entire `messages` array | Partial — Claude CLI always prepends its own session context |
| Tool calls | Not supported (tool definitions are not sent) | Full support — `--allowedTools`, `--disallowedTools`, MCP bridge |
| HITL permission intercept | Not supported | Supported via MCP bridge |
| Session continuity | In-memory; lost if `ail` process exits | Persistent; `--resume <session_id>` uses Claude CLI's own storage |
| Supported models | Any model served by an OpenAI-compatible endpoint | Any model supported by the Claude CLI binary |
| Extended thinking | Via `think: false`/`true` top-level field (Ollama-specific) | Via Claude CLI's native thinking support |

Use the HTTP runner for steps that do not require tool calls and need exact system prompt instructions (classifiers, formatters, judges). Use the Claude CLI runner for agentic steps that require tools, HITL, or persistent cross-session history.

A pipeline may mix both runners freely using the per-step `runner:` field.

### Invocation Model

The HTTP runner sends a single synchronous POST to `{base_url}/chat/completions`. It does not stream — `stream: false` is always sent. The response is returned as a `RunResult` with `response` set to the first choice's message content.

No subprocess is spawned. The entire HTTP lifecycle is handled within the `ail` process using the `ureq` synchronous HTTP client.

### Session Continuity

Session continuity is maintained **in memory** within a shared session store (`HttpSessionStore`) for the lifetime of a single pipeline run:

1. The first call to a given `HttpRunner` with no `resume_session_id` creates a new conversation. A UUID session ID is generated and returned in `RunResult.session_id`.
2. The executor stores this session ID in the `TurnLog` and passes it as `resume_session_id` in subsequent `InvokeOptions`.
3. On resume, `HttpRunner` loads the full message history from the shared store and appends the new user message before the API call.
4. The updated history (including the new assistant response) is written back to the store.

**Shared store:** All `HttpRunner` instances within the same pipeline run share one in-memory conversation store (via `Session.http_session_store`). This means two steps that both declare `runner: ollama` can resume each other's sessions — the store is not per-runner-instance. The store is freed when the pipeline run completes.

**Session IDs are NOT resumable across process restarts.** The `runner_session_id` written to the turn log is a correlation token valid only within the process that created it. If the `ail` process exits (normal completion, crash, or signal), the in-memory history is lost. A consumer reading `ail log` should treat HTTP runner session IDs as opaque — they cannot be passed to a new `ail` invocation to continue the conversation. For cross-process session resumption, use the Claude CLI runner (`--resume <session_id>` persists in Claude CLI's own storage).

**Resume-miss fallthrough:** If `resume_session_id` is set but the session is not found in the store (e.g. because the process restarted), the runner logs a warning and starts a fresh conversation with the system prompt rebuilt from `InvokeOptions`. This prevents silently sending a context-free request.

### Message Structure

The `messages` array sent to the API follows the standard OpenAI format:

```
Fresh conversation:
  [system?, user, ...]                    ← system only if system_prompt: is set

Resumed conversation:
  [system?, user, assistant, user, ...]   ← replayed full history + new user message
```

The system message is built by joining `system_prompt:` (base) and all `append_system_prompt:` entries with `\n\n`. If neither is set, no system message is sent.

### `think` Field

The HTTP runner supports a `think` top-level field in the request body, used by Ollama-hosted models (e.g. `qwen3`) that default to extended thinking mode. To disable thinking:

```bash
AIL_HTTP_THINK=false ail --once "classify this" --pipeline .ail.yaml
```

Or set `think: Some(false)` in `HttpRunnerConfig` when constructing the runner in code.

When `think` is `None` (the default), the field is omitted from the request body and the server decides.

### Configuration

All configuration is via environment variables when using `RunnerFactory`. When constructing `HttpRunner` directly in code, use `HttpRunnerConfig`.

| Env var | Config field | Default | Description |
|---|---|---|---|
| `AIL_HTTP_BASE_URL` | `base_url` | `http://localhost:11434/v1` | Base URL for the OpenAI-compatible API |
| `AIL_HTTP_TOKEN` | `auth_token` | (none) | Bearer token; sent as `Authorization: Bearer <token>` |
| `AIL_HTTP_MODEL` | `default_model` | (none) | Default model name; required unless `InvokeOptions.model` is set |
| `AIL_HTTP_THINK` | `think` | (omitted) | Set to `false` to send `"think": false`; any other value sends `true` |

### Factory Names

`RunnerFactory::build()` recognises two names for the HTTP runner:

| Name | Behaviour |
|---|---|
| `http` | Reads env vars as above; uses `HttpRunnerConfig` defaults where not set |
| `ollama` | Alias for `http`; identical behaviour |

There is no difference between `runner: http` and `runner: ollama` in a pipeline YAML — both resolve to the same `HttpRunner` instance configured from the same env vars.

```yaml
pipeline:
  - id: classify
    prompt: "Classify the above diff as: feature / fix / refactor / docs / chore"
    system_prompt: "You are a commit classifier. Reply with exactly one word."
    runner: ollama      # uses AIL_HTTP_BASE_URL, AIL_HTTP_MODEL, etc.

  - id: implement
    prompt: "Implement the requested change"
    # no runner: — uses the default (claude)
```

### Timeouts

The HTTP runner applies connect and read timeouts to every API call via `ureq::AgentBuilder`:

| Timeout | Default | ProviderConfig field | Description |
|---|---|---|---|
| Connect | 10 seconds | `connect_timeout_seconds` | How long to wait for TCP connection establishment |
| Read | 300 seconds (5 min) | `read_timeout_seconds` | How long to wait for the response body; set generously for slow local models |

Timeouts are configurable per-pipeline and per-step via the standard `ProviderConfig` resolution chain (`defaults:` → per-step → CLI flags). When a timeout fires, the runner returns `AilError::RunnerInvocationFailed` with a transport error detail.

### Cancellation

The HTTP runner supports cooperative cancellation via `InvokeOptions.cancel_token` (`CancelToken`).

The blocking `ureq` call runs in a spawned worker thread. The caller thread waits on an `mpsc` channel. If a `cancel_token` is present, a second thread blocks on the token's `event_listener::EventListener` and sends a `Cancelled` signal on the same channel. Whichever signal arrives first wins — no polling.

On cancellation, the runner returns `AilError::RunnerCancelled` (`ail:runner/cancelled`). The worker thread continues in the background until its request completes or times out (bounded by the read timeout), then its result is silently dropped. The session store is NOT updated on cancellation — no partial history is recorded.

### Context Management and Token Cost

The OpenAI-compatible chat completions API is **stateless** — the client must resend the full `messages` array on every request. When a pipeline uses `resume: true` across N consecutive HTTP runner steps, each step sends the accumulated history:

- Turn 1: ~1 message
- Turn 2: ~3 messages (history + new user message)
- Turn N: ~2N-1 messages

Total input tokens across N resumed steps is **O(N²)** in history length. This is inherent to the stateless API contract, not a bug.

**Recommended pattern:** For HTTP runner steps, prefer explicit context via template variables (`{{ step.<id>.response }}`) over `resume: true`. Each step sends only the context it declares — O(1) per step. Reserve `resume: true` for multi-turn conversations where full history is intentionally required.

**Sliding window (`max_history_messages`):** When set in `ProviderConfig`, the runner truncates the non-system message history to the most recent K messages before each API call. The system prompt (if present) is always preserved. This bounds per-request token cost at O(K) regardless of conversation length.

```yaml
defaults:
  provider:
    max_history_messages: 20   # keep system prompt + last 20 messages
```

### Tool Policy

The HTTP runner ignores `ToolPermissionPolicy`. It never sends tool definitions to the API, so all tool policy values (`RunnerDefault`, `NoTools`, `Allowlist`, etc.) are functionally equivalent. A warning is logged if a policy other than `RunnerDefault` or `NoTools` is set.

### `RunResult` Fields

| Field | Populated | Notes |
|---|---|---|
| `response` | Always | First choice's message content |
| `session_id` | Always | UUID for this conversation; pass as `resume_session_id` to continue |
| `input_tokens` | When returned by API | From `usage.prompt_tokens` |
| `output_tokens` | When returned by API | From `usage.completion_tokens` |
| `model` | When returned by API | From `model` field in response; may differ from requested model |
| `cost_usd` | Never | Not computable without per-model pricing tables |
| `thinking` | Never | Thinking traces are not extracted from the HTTP response |
| `tool_events` | Never | Always empty — HTTP runner does not support tool calls |

### Error Handling

HTTP errors are mapped to `AilError { error_type: "ail:runner/invocation-failed", ... }`:

| Condition | Detail format |
|---|---|
| Non-2xx HTTP response | `"HTTP <code>: <body>"` |
| Transport error (connect refused, timeout) | `"Transport error: <message>"` |
| JSON parse failure on response body | `"HttpRunner: failed to parse response JSON: <error>"` |
| No model specified | `"HttpRunner: no model specified. Set InvokeOptions.model or HttpRunnerConfig.default_model"` |
| Conversation store lock poisoned | `"HttpRunner: conversation store lock poisoned"` |
| Cancelled by `cancel_token` | `"HTTP request cancelled by cancel_token"` (error type: `ail:runner/cancelled`) |
| Worker thread panicked | `"HTTP worker thread terminated unexpectedly"` (error type: `ail:runner/cancelled`) |

### Implementation Notes

- `HttpRunner` is thread-safe — the conversation store is wrapped in `Arc<Mutex<...>>`. Multiple steps in a pipeline execute sequentially (the executor is single-threaded per pipeline), so lock contention is not a concern in practice.
- The session store is scoped to a pipeline run via `Session.http_session_store`. All `HttpRunner` instances in the same run share one store. Per-step `runner: ollama` overrides constructed by the factory receive the same shared store.
- `HttpRunner::new(config, store)` takes an explicit session store. `HttpRunner::ollama(model, store)` is a convenience constructor that sets `base_url = "http://localhost:11434/v1"` and `think = Some(false)`.
- Cancellation uses `event-listener` for event-driven wakeup — no polling. The `CancelToken` type provides `listen() → EventListener` for runners and `cancel()` for callers.
- The runner is implemented in `ail-core/src/runner/http.rs`. All live tests are `#[ignore]` and require a running Ollama instance.

---
