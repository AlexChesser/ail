## 4. The Pipeline Execution Model

### 4.1 `invocation` — Step Zero

Every pipeline has an implicit first step called `invocation`. It is always step zero, always exists, and can always be referenced by subsequent steps via template variables.

`invocation` represents the triggering event and the runner's response to it. The trigger may be:

- A human typing a prompt into the underlying agent
- Another pipeline calling this one as a step
- A scheduled or manual trigger

The pipeline's authored steps begin executing only after `invocation` completes. `ail` never intercepts or replaces the triggering interaction — it extends it.

```
invocation           ← step zero; always present
  ↓
step_1               ← first authored step in the pipeline
  ↓
step_2
  ↓
  ...
  ↓
[control returns to caller]
```

#### Declaring `invocation` in YAML

`invocation` may be declared explicitly as the first step in the pipeline YAML. When declared, it must be the first step; placing it anywhere else is a validation error.

```yaml
version: "0.0.1"
pipeline:
  - id: invocation
    prompt: "{{ session.invocation_prompt }}"
  - id: review
    prompt: "Review the above output."
```

**If `invocation` is declared**, the executor runs it as a first-class step with whatever configuration the user supplied — this includes a custom prompt template, a non-default model or provider, `before:`/`then:` hooks, or any other step-level configuration. The host does not run a separate default invocation.

**If `invocation` is not declared**, the host runs a default invocation (the `--once` prompt, plain settings) before handing off to the executor. This is the minimal case for pipelines that do not need to customise how the triggering interaction is handled.

Declaring `invocation` in YAML also makes it visible in `materialize` output, which is the primary way readers understand what a pipeline does end-to-end.

#### `invocation` in `FROM` chains

When a pipeline inherits from a base pipeline via `FROM`, the `invocation` step belongs to the triggering pipeline — not the base. The inherited steps execute after `invocation` completes, in the order they appear in the resolved (materialised) pipeline. The `invocation` step is never inherited and never duplicated; it fires exactly once per pipeline run, always as step zero.

Because `invocation` names the event rather than the actor, the template variables are unambiguous regardless of what triggered the pipeline:

- `{{ step.invocation.prompt }}` — the input that triggered this pipeline run
- `{{ step.invocation.response }}` — the runner's response before any pipeline steps ran

### 4.2 Execution Guarantee

Once an `invocation` completion event fires, `ail` begins executing the pipeline before control returns to the caller. If a HITL gate fires mid-pipeline, control remains locked until the human responds. Individual steps may be skipped by declared conditions, and execution may terminate early via `break`, `abort_pipeline`, or an unhandled error — all of which are explicit, declared outcomes recorded in the pipeline run log.

### 4.3 Hooks on `invocation`

Hook operations may target `invocation` directly, enabling session setup before the first prompt is processed.

```yaml
- run_before: invocation
  id: session_banner
  action: pause_for_human
  message: "Reminder: all outputs in this session are subject to compliance review."
```

The `before:` chain on `invocation` is a more powerful variant: rather than inserting a new step adjacent to invocation, it attaches private pre-processing that can transform the user's prompt before it reaches the agent. See §5.7 for full documentation and the governance warnings that apply when using this in a `FROM` base pipeline.

### 4.4 Pipeline Run Log & Step Context

Every pipeline execution is backed by a **pipeline run log** — a durable, structured record written to disk before the next step begins. The log is the authoritative source for template variable resolution. An implementation that resolves template variables from an in-memory cache without a durable backing store does not conform to this spec.

Each log entry must be flushed to the storage layer (`fsync`/`sync_data` equivalent) before execution continues. An implementation that buffers log entries without flushing does not conform to this spec. When a composite (multi-backend) log provider is used, the entry is considered durably recorded if at least one backend succeeds; all individual backend failures are logged as warnings, and the run is only aborted if all backends fail.

#### Log Identity

Each pipeline run is identified by a `pipeline.run_id` — the same identifier used in the tracing and observability systems (see §22). There is no separate context identifier; run identity is unified across logging, tracing, and template variable access.

#### Log Location

Run logs are stored per project, not per invocation. The project is identified by a SHA-1 hash of the working directory path at session start:

```
~/.ail/projects/<sha1_of_cwd>/runs/<run_id>.jsonl
```

This means all `ail` runs within the same working directory share a project bucket. A new `--once` invocation in the same repository automatically has access to the full history of prior runs in that project. Starting a clean session in the same project is a deferred feature (see §22).

#### Step Event Sequence

Two events are written to the log per step:

1. **`step_started`** — written immediately before the runner is invoked. Contains `step_id` and the fully resolved `prompt`. If the runner crashes or hangs, this record is the only evidence the step was attempted.
2. **`step_completed`** (a full `TurnEntry`) — written when the runner returns a response. Contains `step_id`, `prompt`, `response`, `cost_usd`, and `runner_session_id`.

An implementation that writes only on completion does not conform to this spec. The `step_started` event is required for crash-safe observability.

#### What Is Logged Per Step

Each completed step's log entry captures, at minimum:

| Field | Always present | Notes |
|---|---|---|
| `prompt` | Yes | The prompt sent to the LLM for this step |
| `response` | Yes | The final text output of the step |
| `tool_calls[]` | If any occurred | Name, input parameters, and result per call |
| `interim_calls[]` | If provider exposes them | Mid-step LLM calls where available |
| `provider` | Yes | Which provider handled this step |
| `session_id` | If provider reports it | Captured for session resumption. |
| `cost_usd` | If provider reports it | Token cost for this step |
| `duration_ms` | Yes | Wall clock time for the step |
| `condition_result` | If condition declared | Whether the step's condition evaluated true or false |
| `on_result_matched` | If `on_result` declared | Which branch fired |
| `error` | If step failed | Structured error detail: error_type, title, detail. Follows the RFC 9457-inspired AilError model defined in ARCHITECTURE.md. null on success. |

#### Accessing Prior Step Results

Any step may access the logged output of any previously completed step in the same pipeline run via template variables:

```
{{ step.invocation.prompt }}           — the original human prompt
{{ step.invocation.response }}         — the runner's response
{{ step.dry_refactor.response }}       — a named step's response
{{ step.dry_refactor.tool_calls }}     — a named step's tool calls (array)
{{ last_response }}                    — the immediately preceding step's response
```

Variables resolve at step execution time from the persisted log, not from in-memory state. A reference to a step that has not yet run raises a fatal parse error. A reference to a step that was skipped by its condition raises a fatal parse error unless the referencing step has a matching condition guard, in which case it resolves to an empty string.

#### Provider Isolation

Steps running against different providers are isolated from each other by default. Each step calling a different provider receives only the context explicitly injected via template variables — there is no implicit cross-provider session sharing.

Steps running against the same provider also run in isolation by default — each step is a fresh invocation. Session continuity within the same provider must be explicitly requested via `resume: true` (see §15.4).

#### Sub-Pipeline Context Isolation

A called pipeline (via `pipeline:` step) owns its context in isolation. The caller has access only to the sub-pipeline's input, final response, and — where available — its top-level tool calls. The sub-pipeline's internal steps, intermediate responses, and local template variables are not visible to the caller.

<!-- compact:skip -->
### §4.5 Controlled Execution Mode

> **Consumer documentation:** §23 (Structured Output & Controlled Mode) documents the same event stream and stdin control protocol from the consumer's perspective. §4.5 is the implementation source of truth; §23 extends it with consumer-oriented detail. Keep both in sync when adding or changing event types.

> **Implementation status:** v0.1 — fully implemented. Used by `--output-format json` and TUI mode.

#### When It Is Used

Two execution paths exist in the binary:

1. **`execute()`** — the simple path used by `--once --output-format text` (default). Runs all steps in order, returns `ExecuteOutcome`. No streaming, no pause/kill, no HITL gate support.
2. **`execute_with_control()`** — the controlled path used by `--output-format json` and the TUI. Streams `ExecutorEvent`s through an mpsc channel; respects kill/pause signals between steps; blocks on HITL gates until the caller sends a response.

Both paths call the same step execution logic; `execute_with_control` is an additive layer, not a replacement.

#### `ExecutionControl` Struct

```rust
pub struct ExecutionControl {
    /// Set to `true` to request a pause between steps.
    /// The executor spin-waits (50ms interval) until cleared or kill is set.
    pub pause_requested: Arc<AtomicBool>,
    /// Set to `true` to request that the executor stop immediately after the current step.
    pub kill_requested: Arc<AtomicBool>,
    /// Callback for tool permission HITL via the MCP bridge (SPEC §13.3).
    /// Propagated into `InvokeOptions::permission_responder` for each runner invocation.
    pub permission_responder: Option<PermissionResponder>,
}
```

The caller shares these `Arc` values with the stdin reader thread (in JSON mode) or with the TUI event loop, which sets/clears the flags in response to user input or control messages.

`kill_requested` is also passed as `InvokeOptions::cancel_token` so the runner can abort an in-flight invocation.

#### `ExecutorEvent` Enum

Every state change is emitted as one of the following variants. In JSON mode, each is serialised as a single NDJSON line with `"type"` as the discriminant (`#[serde(tag = "type", rename_all = "snake_case")]`).

| Variant | `"type"` in JSON | When emitted |
|---|---|---|
| `StepStarted` | `"step_started"` | Before the runner is called; `resolved_prompt` is `None` for non-prompt steps |
| `StepCompleted` | `"step_completed"` | After the runner returns; `response` is `None` for context/action/sub-pipeline steps |
| `StepSkipped` | `"step_skipped"` | When a step is in the `disabled_steps` set |
| `StepFailed` | `"step_failed"` | When a step errors (includes `error` string detail) |
| `HitlGateReached` | `"hitl_gate_reached"` | When a `pause_for_human` step is reached; executor blocks until `hitl_rx` receives a value |
| `HitlModifyReached` | `"hitl_modify_reached"` | When a `modify_output` step is reached (§13.2); includes `last_response` for the human to edit; blocks until `hitl_rx` receives the modified text |
| `RunnerEvent` | `"runner_event"` | Wraps a streaming `RunnerEvent` (thinking, stream delta, tool call, etc.) as `"event"` field |
| `PipelineCompleted` | `"pipeline_completed"` | When all steps complete; `"outcome"` field is `"completed"` or `"break"` |
| `PipelineError` | `"pipeline_error"` | When the pipeline aborts due to an error; includes `"error"` and `"error_type"` |

**`StepStarted` payload:**
```json
{"type": "step_started", "step_id": "review", "step_index": 1, "total_steps": 3, "resolved_prompt": "Review the above output."}
```
For non-prompt steps (context:shell, action, sub-pipeline), `resolved_prompt` is omitted.

**`StepCompleted` payload:**
```json
{"type": "step_completed", "step_id": "review", "cost_usd": 0.0012, "input_tokens": 120, "output_tokens": 85, "response": "...", "model": "claude-opus-4-5"}
```
For non-prompt steps, `response` is omitted.

**`RunnerEvent` payload:**
```json
{"type": "runner_event", "event": {"type": "stream_delta", "text": "partial response text"}}
```
The inner `"event"` object uses the `RunnerEvent` JSON schema (e.g., `"type": "thinking"`, `"type": "stream_delta"`).

**`HitlGateReached` payload:**
```json
{"type": "hitl_gate_reached", "step_id": "review_gate", "message": "Please review before continuing."}
```
The `"message"` field is omitted when not declared in the step YAML.

**`HitlModifyReached` payload (§13.2):**
```json
{"type": "hitl_modify_reached", "step_id": "review_gate", "message": "Edit the output.", "last_response": "Generated document text..."}
```
`"message"` and `"last_response"` are omitted when not available. The consumer sends a `hitl_response` message with the edited text to unblock the executor.

#### Function Signature

```rust
pub fn execute_with_control(
    session: &mut Session,
    runner: &dyn Runner,
    control: &ExecutionControl,
    disabled_steps: &HashSet<String>,
    event_tx: mpsc::Sender<ExecutorEvent>,
    hitl_rx: mpsc::Receiver<String>,
) -> Result<ExecuteOutcome, AilError>
```

- `disabled_steps` — set of step IDs to skip with a `StepSkipped` event; used by the TUI to let users deselect individual steps before running.
- `event_tx` — unbounded mpsc sender; the caller drains it (on the same or a separate thread) as events arrive.
- `hitl_rx` — mpsc receiver; the executor blocks here when a `pause_for_human` step fires. The caller must send any string to resume.

`execute_with_control` emits a `PipelineCompleted` or `PipelineError` event before returning, so callers that only consume the event channel still receive a definitive terminal event.

#### NDJSON Stdin Control Protocol (JSON Mode)

When running with `--output-format json`, `ail` spawns a stdin reader thread that routes NDJSON control messages into the executor's control channels. Consumers (e.g. the VS Code extension) write one JSON object per line to the process stdin.

**Message types:**

| `"type"` | Fields | Effect |
|---|---|---|
| `"hitl_response"` | `"text": string` | Sends `text` to `hitl_rx`, unblocking a `pause_for_human` gate |
| `"permission_response"` | `"allowed": bool`, `"reason": string` (when `allowed` is `false`) | Delivers a tool permission decision to the pending `PermissionResponder` callback |
| `"pause"` | — | Sets `pause_requested = true`; executor stops between steps |
| `"resume"` | — | Clears `pause_requested`; executor continues from where it paused |
| `"kill"` | — | Sets `kill_requested = true`; executor stops after the current step and does not start the next |

**Permission response flow:**

1. The runner intercepts a tool permission request from the agent.
2. The `PermissionResponder` callback fires, parking a `SyncSender` in a shared mutex and blocking for up to 300 seconds.
3. The runner emits a `"runner_event"` with type `"permission_request"` to the consumer.
4. The consumer sends a `"permission_response"` message on stdin.
5. The stdin reader picks up the sender from the mutex and delivers the `PermissionResponse`.
6. The callback returns; the runner proceeds.

**Examples:**

```json
{"type": "hitl_response", "text": "Looks good, continue."}
{"type": "permission_response", "allowed": true}
{"type": "permission_response", "allowed": false, "reason": "Denied: sensitive file."}
{"type": "pause"}
{"type": "resume"}
{"type": "kill"}
```

#### Run Started Event

Before any steps execute, `run_once_json` emits a `run_started` envelope:

```json
{"type": "run_started", "run_id": "<uuid>", "pipeline_source": "/path/to/.ail.yaml", "total_steps": 3}
```

`pipeline_source` is `null` when in passthrough mode (no pipeline file found).
<!-- /compact:skip -->

---
