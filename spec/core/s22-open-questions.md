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

**Note:** This may be a tool implementation decision rather than a spec decision. The spec defines what pipelines *are*; whether the runtime watches for file changes is an operational concern. Flagged here until implementation experience clarifies whether it needs to be specced.

---

### Skill Parameterisation

**Question:** How does a `SKILL.md` declare what parameters it accepts, and how are they injected — as template variables, environment variables, or a structured input block?

**Status:** Deferred. The `with:` syntax has been removed from the spec pending this design. Will be revisited when structured I/O schema support (§22) is implemented.

---

*This is a living document. Open a PR against this file to propose changes to the spec.*
