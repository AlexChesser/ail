## 9. Calling Pipelines as Steps

A pipeline may call another as a step using the `pipeline:` primary field.

### 9.1 Isolation Model

```
Caller pipeline context
  ↓ (full current context passed as input)
Called pipeline
  └─ invocation = caller's current context snapshot
  └─ runs its own steps in complete isolation
  └─ its own template variables are all local
  └─ returns its final step's output as a single response
  ↓
Caller receives {{ step.<call_id>.response }}
```

The caller sees only the called pipeline's final output. Internal steps, intermediate responses, and local context are not visible to the caller.

### 9.2 Syntax

```yaml
- id: run_security_suite
  pipeline: ./pipelines/security-suite.yaml
  on_result:
    contains: "ALL_CLEAR"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security suite found issues."
```

### 9.3 Failure Propagation

If the called pipeline aborts internally, `ail` surfaces a pipeline stack trace to the TUI — equivalent to a call stack — showing which pipeline failed, at which step, and why. The caller's `on_error` field governs what happens next. The full internal trace is written to the pipeline run log.

---
