# 23. Structured Output Mode

## 23.1 Purpose

`ail` supports a structured output mode that emits pipeline execution events as NDJSON (newline-delimited JSON) to stdout. This enables programmatic consumers — VS Code extensions, CI systems, monitoring tools, and other agent orchestrators — to observe and react to pipeline execution in real time without parsing human-readable text.

## 23.2 Activation

Structured output is activated via the `--output-format` CLI flag:

```bash
ail --once "refactor auth" --pipeline .ail.yaml --output-format json
```

| Value | Behaviour |
|-------|-----------|
| `text` | Default. Human-readable text output (unchanged from v0.0.1). |
| `json` | NDJSON event stream to stdout. One JSON object per line. |

When `--output-format json` is active:
- All `ExecutorEvent`s are serialized as JSON lines to stdout.
- Tracing/diagnostic output continues to go to stderr (unchanged).
- The process exit code is 0 on success, 1 on pipeline error.

## 23.3 Event Schema

Every event is a JSON object with a `"type"` field that identifies the event kind. Events are emitted in execution order.

### Envelope Events

**`run_started`** — first event, emitted before any steps execute:
```json
{
  "type": "run_started",
  "run_id": "a1b2c3d4-...",
  "pipeline_source": ".ail.yaml",
  "total_steps": 3
}
```

### Executor Events

These mirror the `ExecutorEvent` enum in `ail-core/src/executor.rs`.

**`step_started`**:
```json
{
  "type": "step_started",
  "step_id": "review",
  "step_index": 0,
  "total_steps": 3,
  "resolved_prompt": "Please review the following code for correctness..."
}
```

`resolved_prompt` is the fully-resolved prompt text (all `{{ variable }}` substitutions applied) that will be sent to the runner. It is `null` for non-prompt steps (`context:shell`, `action`, sub-pipeline). For prompt steps, `step_started` is emitted **after** template resolution so `resolved_prompt` is always populated when present.

**`step_completed`**:
```json
{
  "type": "step_completed",
  "step_id": "review",
  "cost_usd": 0.003,
  "input_tokens": 1234,
  "output_tokens": 567,
  "response": "The code looks well-structured. A few observations..."
}
```

`cost_usd` is `null` for non-runner steps (context:shell, pause_for_human, sub-pipeline).
`input_tokens` and `output_tokens` are `0` for non-runner steps.
`response` is the runner's full response text. It is `null` for non-prompt steps.

**`step_skipped`** — step was disabled or skipped by control logic:
```json
{
  "type": "step_skipped",
  "step_id": "optional_check"
}
```

**`step_failed`**:
```json
{
  "type": "step_failed",
  "step_id": "review",
  "error": "Template variable 'step.missing.response' not found"
}
```

**`hitl_gate_reached`** — a `pause_for_human` step is blocking:
```json
{
  "type": "hitl_gate_reached",
  "step_id": "human_review"
}
```

**`pipeline_completed`**:
```json
{
  "type": "pipeline_completed",
  "outcome": "completed"
}
```

Or with a break:
```json
{
  "type": "pipeline_completed",
  "outcome": "break",
  "step_id": "early_exit"
}
```

**`pipeline_error`**:
```json
{
  "type": "pipeline_error",
  "error": "Step 'deploy' on_result fired abort_pipeline",
  "error_type": "ail:pipeline/aborted"
}
```

### Runner Events

Runner events are nested under `"type": "runner_event"` with the runner event in the payload.

**`stream_delta`** — incremental text from the model:
```json
{
  "type": "runner_event",
  "event": { "type": "stream_delta", "text": "Hello" }
}
```

**`thinking`** — extended thinking block:
```json
{
  "type": "runner_event",
  "event": { "type": "thinking", "text": "Let me consider..." }
}
```

**`tool_use`** — tool invocation started:
```json
{
  "type": "runner_event",
  "event": { "type": "tool_use", "tool_name": "Bash" }
}
```

**`tool_result`** — tool invocation completed:
```json
{
  "type": "runner_event",
  "event": { "type": "tool_result", "tool_name": "Bash" }
}
```

**`cost_update`** — token and cost metrics:
```json
{
  "type": "runner_event",
  "event": { "type": "cost_update", "cost_usd": 0.012, "input_tokens": 100, "output_tokens": 50 }
}
```

**`permission_requested`** — tool permission decision needed:
```json
{
  "type": "runner_event",
  "event": { "type": "permission_requested", "display_name": "Bash", "display_detail": "rm -rf /tmp/test" }
}
```

**`completed`** — runner invocation finished:
```json
{
  "type": "runner_event",
  "event": { "type": "completed", "response": "Done.", "cost_usd": 0.01, "session_id": "ses_123" }
}
```

## 23.4 Event Ordering Guarantees

1. `run_started` is always the first event.
2. For each step: `step_started` is emitted before any runner events for that step.
3. `step_completed` or `step_failed` is emitted after all runner events for that step.
4. `pipeline_completed` or `pipeline_error` is always the last executor event.
5. Runner events (`stream_delta`, `tool_use`, etc.) are emitted between `step_started` and `step_completed` for the active step.
6. For prompt steps, `step_started` is emitted after template resolution, so `resolved_prompt` is always populated. If template resolution fails, `step_failed` is emitted without a preceding `step_started`.

## 23.5 Interaction with Other Flags

| Flag combination | Behaviour |
|-----------------|-----------|
| `--output-format json --headless` | NDJSON output, no tool permission prompts (auto-skip). |
| `--output-format json` (no `--headless`) | NDJSON output, tool permissions via MCP bridge. |
| `--output-format text` | Default text output — prints final response(s) only. |
| `--output-format text --show-thinking` | Text output with per-step thinking blocks printed to stderr. |
| `--output-format text --show-responses` | Text output with per-step response blocks printed to stderr. |
| `--output-format text --show-thinking --show-responses` | Both thinking and response blocks per step. |

`--show-thinking` and `--show-responses` are only meaningful with `--output-format text` and `--once`. They have no effect in JSON mode (thinking/response events are already in the NDJSON stream for consumers to handle).

## 23.6 Implementation Status

| Feature | Status |
|---------|--------|
| `--output-format` CLI flag | **v0.1** ✓ |
| `ExecutorEvent` serialization | **v0.1** ✓ |
| `RunnerEvent` serialization | **v0.1** ✓ |
| `run_started` envelope event | **v0.1** ✓ |
| stdin HITL response channel | planned (Phase 4) |
| stdin permission response channel | planned (Phase 4) |
