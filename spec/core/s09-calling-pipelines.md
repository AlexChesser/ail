## 9. Calling Pipelines as Steps

> **Implementation status:** Implemented. Sub-pipeline isolation, depth guards (MAX_SUB_PIPELINE_DEPTH = 16), template variable resolution in pipeline paths, failure propagation, `on_result: pipeline:` action, and `prompt:` override all work. See `ail-core/src/executor/headless.rs` for the implementation.

A pipeline may call another as a step using the `pipeline:` primary field.

### 9.1 Isolation Model

```
Caller pipeline context
  ↓ (invocation prompt: caller's last_response, OR explicit prompt: override)
Called pipeline
  └─ invocation = the passed prompt (see §9.3)
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

### 9.3 Invocation Prompt Override

By default, a sub-pipeline's `invocation_prompt` (the value of `{{ step.invocation.prompt }}` inside the child) is set to the parent's most recent step response. This passes the previous step's output into the child naturally.

When the calling context needs to pass something other than the last response — for example, the original user prompt rather than an intermediate classification — use the optional `prompt:` field:

```yaml
# Inline pipeline: step
- id: implement
  pipeline: ./agents/hephaestus.ail.yaml
  prompt: "{{ step.invocation.prompt }}"

# on_result: pipeline: action
on_result:
  - contains: "EXPLICIT"
    action: "pipeline: ./workflows/explicit.ail.yaml"
    prompt: "{{ step.invocation.prompt }}"
```

`prompt:` is a template string resolved against the **parent** session at execution time. Any template variable valid in the parent is valid here. The resolved value becomes the child session's `invocation_prompt`.

When `prompt:` is omitted, the default is `session.turn_log.last_response() ?? session.invocation_prompt`.

### 9.3 Failure Propagation

If the called pipeline aborts internally, `ail` surfaces a pipeline stack trace to the TUI — equivalent to a call stack — showing which pipeline failed, at which step, and why. The caller's `on_error` field governs what happens next. The full internal trace is written to the pipeline run log.

### 9.4 Depth Guard

Sub-pipeline nesting is limited to `MAX_SUB_PIPELINE_DEPTH = 16` levels. If a chain of `pipeline:` steps (or `on_result: pipeline:` actions) would exceed this depth, execution aborts with a `PIPELINE_ABORTED` error before the offending step runs.

This guard applies equally to both execution paths:
- `execute()` — simple mode (`--once`)
- `execute_with_control()` — controlled mode (TUI, `--output-format json`)

A sub-pipeline called from a top-level pipeline starts at depth 1. Each further level increments the counter. Depth 0 is reserved for `execute_inner` when called directly from `execute()` or `execute_with_control()`.

---
