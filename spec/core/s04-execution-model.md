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

---
