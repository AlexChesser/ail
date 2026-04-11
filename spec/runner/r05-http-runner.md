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

Session continuity is maintained **in memory** within the `HttpRunner` instance for the lifetime of the `ail` process:

1. The first call to a given `HttpRunner` with no `resume_session_id` creates a new conversation. A UUID session ID is generated and returned in `RunResult.session_id`.
2. The executor stores this session ID in the `TurnLog` and passes it as `resume_session_id` in subsequent `InvokeOptions`.
3. On resume, `HttpRunner` loads the full message history from its in-memory store and appends the new user message before the API call.
4. The updated history (including the new assistant response) is written back to the in-memory store.

**Persistence caveat:** The conversation history is not written to disk. If the `ail` process exits (normal completion, crash, or signal), the in-memory history is lost. For pipelines that need to resume across separate `ail` invocations, use the Claude CLI runner (`--resume <session_id>` persists in Claude CLI's own storage).

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

### Implementation Notes

- `HttpRunner` is thread-safe — the conversation store is wrapped in `Arc<Mutex<...>>`. Multiple steps in a pipeline execute sequentially (the executor is single-threaded per pipeline), so lock contention is not a concern in practice.
- `HttpRunner::ollama(model)` is a convenience constructor that sets `base_url = "http://localhost:11434/v1"` and `think = Some(false)`. It is the recommended entry point for local Ollama pipelines in code.
- The runner is implemented in `ail-core/src/runner/http.rs`. All live tests are `#[ignore]` and require a running Ollama instance. Run with `cargo nextest run -- --include-ignored` on a host with Ollama running.

---
