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
| `continue` | Log error, proceed. Only for explicitly non-critical steps. |
| `pause_for_human` | Suspend pipeline, surface error in HITL panel. **Default.** |
| `abort_pipeline` | Stop immediately. Log full error context to pipeline run log. |
| `retry` | Retry up to `max_retries` times, then escalate to `pause_for_human`. |

```yaml
defaults:
  on_error: pause_for_human

pipeline:
  - id: optional_style_check
    on_error: continue
    prompt: "Check for style guide violations."

  - id: critical_security_scan
    on_error: abort_pipeline
    max_retries: 3
    prompt: "Scan for security vulnerabilities."
```

---
