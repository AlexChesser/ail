## 22. Open Questions

These are unresolved questions that require either implementation experience or dedicated research before they can be specced. They are tracked here so they are not lost.

---

### Completion Detection

**Status: Resolved for Claude CLI.**

The Claude CLI `--output-format stream-json` flag produces a newline-delimited NDJSON stream. Completion is signalled by a `{"type": "result", "subtype": "success", ...}` event — unambiguous, structured, and carrying cost metadata. PTY wrapping is not required for the Claude CLI runner.

For other runners without a structured output mode, process exit code 0 remains the fallback hypothesis. This should be validated per runner during each integration sprint. See `RUNNER-SPEC.md`.

**Remaining work:** Verify that `--output-format stream-json` is available in all Claude CLI invocation modes used by `ail`, and document error event shapes (`subtype: error`) for `on_error` handling.

---

### Context Accumulation

**Status: Resolved. See §4.4.**

The pipeline run log (§4.4) is the context system. Steps access prior results by querying the persisted log via template variables. Provider isolation is the default; session continuity is opt-in via `resume: true` (§15.4). The spike must validate the exact mechanics of `--input-format stream-json` for same-provider session resumption.

**Remaining work:** Spike must determine whether `--input-format stream-json` supports sending a new pipeline step prompt within the same session, or whether each step requires a new subprocess invocation with context passed via `{{ step.invocation.response }}` and `{{ last_response }}` template variables.

---

### Step Turns & Structured Output Data Model

**Status: Concrete model established for Claude CLI. Full spec deferred.**

The `--output-format stream-json` stream provides structured event types that map directly to the proposed `turns[]` model:

```
step.<id>
  .response              ← content of the result event; flows to next step
  .cost_usd              ← total_cost_usd from result event
  .turns[]               ← full NDJSON event sequence
    .type                ← "assistant" | "user" | "system"
    .content[]
      .type              ← "tool_use" | "tool_result" | "text"
      .name              ← tool name (tool_use events)
      .input             ← tool input parameters (tool_use events)
      .result            ← tool result (tool_result events)
      .text              ← text content (text events)
```

Full speccing of `step.<id>.turns[]` template variable access is deferred until the spike validates the exact event shapes and confirms whether partial message streaming (`--include-partial-messages`) is needed for the MVP.

**Remaining work:** Spike validation of event shapes, especially error events and extended thinking blocks.

---

### Hot Reload

**Question:** If a user edits `.ail.yaml` while a session is running, does the change take effect immediately or require a session restart?

**Extended question (self-modifying pipelines):** The `apply_pipeline_diff` action (§21) writes a modification to the active pipeline file. What is the timing guarantee — can hot reload take effect mid-run (affecting steps later in the same execution), or only at the boundary between runs?

The mid-run case creates a consistency risk: a step that fires after the reload was not declared in the pipeline that started the run. The between-runs case is safer but limits the utility of immediate self-improvement within a session.

**Note:** This may be a tool implementation decision rather than a spec decision. The spec defines what pipelines *are*; whether the runtime watches for file changes is an operational concern. The self-modifying pipeline case requires an explicit decision — the diff action cannot be useful without a defined reload contract. Flagged here until implementation experience clarifies both paths.

---

### Self-Modifying Pipeline: Approval Flow

**Question:** `pause_for_human` (§13) is the HITL gate for standard step review. Pipeline modification is a different category of approval — the human is approving a change to the control plane itself, not to the output of a task. Should this be:

1. A standard `pause_for_human` where the message conveys the nature of the change (simpler, but no runtime distinction between types of approval)?
2. A dedicated action — e.g., `approve_pipeline_modification` — with specific display semantics: renders the YAML diff, shows the target file path, and requires confirmed intent before writing?

Option 2 is safer for interactive use (harder to accidentally approve a structural change), but adds a new action type that interacts with `on_result` matching. The open question is whether the runtime needs to distinguish between "approve this output" and "approve writing this file" at a structural level, or whether that distinction lives entirely in the UX layer.

**Note:** Until this is resolved, the design seed in §21 uses `pause_for_human` as a proxy — this is accurate enough for the vision to be understood, but is not the final form.

---

### Self-Modifying Pipeline: Diff Validation

**Question:** Before `apply_pipeline_diff` writes to disk, the runtime must confirm the result is a valid pipeline. How thorough should this validation be, and what happens on failure?

- **Syntactic validation** — the diff parses as valid YAML (necessary, not sufficient)
- **Schema validation** — the result conforms to the pipeline schema (catches structural errors)
- **Semantic validation** — the result does not introduce circular `FROM` references, references non-existent step IDs, or declares conditions that can never be satisfied (higher confidence, higher implementation cost)

A diff produced by an LLM is an untrusted input. Applying it directly without validation risks writing a malformed or semantically invalid pipeline that breaks all subsequent invocations. The spec must decide: what level of validation is required before write, and does validation failure escalate via `on_error` or via a dedicated rejection path?

---

### Skill Parameterisation

**Question:** How does a `SKILL.md` declare what parameters it accepts, and how are they injected — as template variables, environment variables, or a structured input block?

**Status:** Deferred. The `with:` syntax has been removed from the spec pending this design. Will be revisited when structured I/O schema support (§22) is implemented.

---

*This is a living document. Open a PR against this file to propose changes to the spec.*
