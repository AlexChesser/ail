## 16. Error Handling & Resilience

### What Constitutes an Error

`on_error` fires on execution failures — conditions where the step could not produce a result:

| Error condition | Examples |
|---|---|
| Runner crash or non-zero process exit | Claude CLI exits 1, OOM, killed |
| Timeout | Step exceeded `timeout_seconds` |
| Network failure | Provider unreachable, TLS error |
| Shell process failed to start | Command not found, fork failure |
| Template resolution failure | Unresolved `{{ variable }}` reference |

**Non-zero exit codes from `context: shell:` steps are not errors.** A shell command that exits 1 has completed successfully from `ail`'s perspective — `on_result` fires with the captured output and exit code. `on_error` does not fire.

An empty LLM response is not an error. It is a result (potentially matched by `on_result: is_empty:`).

### `on_error` / `on_result` Lifecycle

These two mechanisms are mutually exclusive within a single step execution:

1. Step runs.
2. **If the step errors** (see above) → `on_error` fires. `on_result` does **not** fire.
3. **If the step completes** (runner returns, any content, any exit code) → `on_result` fires. `on_error` does **not** fire.
4. `repeat_step` (from `on_result`) and `on_error: retry` share the same `max_retries` counter.

### `on_error` Values

| Value | Effect |
|---|---|
| `abort_pipeline` | Stop immediately. Log full error context to pipeline run log. **Default.** |
| `continue` | Log error, proceed to next step. No turn entry is recorded for the failed step. Only for explicitly non-critical steps. |
| `retry` | Retry up to `max_retries` times (required field), then abort pipeline. |

When `on_error` is not specified, the default behaviour is `abort_pipeline`.

```yaml
pipeline:
  - id: optional_style_check
    on_error: continue
    prompt: "Check for style guide violations."

  - id: critical_security_scan
    on_error: retry
    max_retries: 3
    prompt: "Scan for security vulnerabilities."

  - id: deploy
    on_error: abort_pipeline
    prompt: "Deploy the application."
```

### Validation Rules

- `on_error: retry` **requires** `max_retries` (integer >= 1). Missing or zero `max_retries` is a `CONFIG_VALIDATION_FAILED` error.
- `max_retries` without `on_error: retry` is a `CONFIG_VALIDATION_FAILED` error.
- Unknown `on_error` values are a `CONFIG_VALIDATION_FAILED` error.

### Retry Semantics

When `on_error: retry` is set and a step fails:

1. The executor records a `step_error` event to the turn log with `on_error_action: "retry"`, `retry_attempt`, and `max_retries`.
2. The step is re-executed from scratch (template resolution, runner invocation, etc.).
3. If the step succeeds on any attempt, execution continues normally — the turn entry from the successful attempt is recorded and `on_result` evaluation proceeds.
4. If all `max_retries` attempts are exhausted (total attempts = `max_retries + 1`), the last error is propagated and the pipeline aborts.

### Turn Log Events

Error handling actions produce `step_error` events in the NDJSON turn log:

```json
{"type": "step_error", "step_id": "s1", "error_type": "ail:runner/invocation-failed", "error_detail": "...", "on_error_action": "continue"}
{"type": "step_error", "step_id": "s1", "error_type": "ail:runner/invocation-failed", "error_detail": "...", "on_error_action": "retry", "retry_attempt": 1, "max_retries": 3}
```

### Executor Events

For controlled mode (`--output-format json`), two additional event types are emitted:

| Event | When |
|---|---|
| `step_error_continued` | `on_error: continue` swallowed the error |
| `step_retrying` | `on_error: retry` is retrying the step |

### Stable `error_type` Values

Every `AilError` carries a stable `error_type` string used in NDJSON output and consumed by downstream tooling. These values must not change across releases.

| Constant | Value | When produced |
|---|---|---|
| `CONFIG_INVALID_YAML` | `ail:config/invalid-yaml` | YAML parse failure |
| `CONFIG_FILE_NOT_FOUND` | `ail:config/file-not-found` | Pipeline or prompt file missing |
| `CONFIG_VALIDATION_FAILED` | `ail:config/validation-failed` | Pipeline fails structural validation (including unimplemented step types — see below) |
| `TEMPLATE_UNRESOLVED` | `ail:template/unresolved-variable` | Template variable cannot be resolved |
| `RUNNER_INVOCATION_FAILED` | `ail:runner/invocation-failed` | Runner subprocess or HTTP call failed |
| `RUNNER_CANCELLED` | `ail:runner/cancelled` | Runner was cancelled via cancel token |
| `RUNNER_NOT_FOUND` | `ail:runner/not-found` | No runner registered for the requested name |
| `PIPELINE_ABORTED` | `ail:pipeline/aborted` | `abort_pipeline` action fired, or unrecoverable runtime error |
| `STORAGE_QUERY_FAILED` | `ail:storage/query-failed` | SQLite or JSONL read error in the log/query layer |
| `RUN_NOT_FOUND` | `ail:storage/run-not-found` | Requested run ID does not exist in the database or JSONL store |
| `STORAGE_DELETE_FAILED` | `ail:storage/delete-failed` | SQLite delete or JSONL file removal failed |

### Unimplemented Step Types

Step types that are declared in the spec but not yet implemented in the current version are rejected at pipeline **load time** (validation), not at execution time. This ensures users see a clear `CONFIG_VALIDATION_FAILED` error immediately rather than a runtime abort.

Currently unimplemented step types:
- `skill:` — planned for v0.2+. Use `pipeline:` steps to compose pipelines.

---
