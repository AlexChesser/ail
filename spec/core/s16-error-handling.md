## 16. Error Handling & Resilience

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
