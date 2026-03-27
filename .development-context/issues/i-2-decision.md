# Decision: i-2 — Pipeline-Level Switching

**Status:** Resolved
**Date:** 2026-03-27
**Resolves:** i-2 (Spec Impact Analysis — Pipeline-Level Switching Capability)
**Relates to:** i-1 (TUI Pipeline Switching), D-019 (Self-Modifying Pipelines)

---

## Decision

**Scenario A** — no new `switch_pipeline` directive needed.

The existing spec primitives cover pipeline-initiated delegation:

1. **`pipeline:` step body** (§5, §9) — sub-pipeline call with isolation
2. **`on_result: pipeline: <path>`** (§5.4) — conditional delegation based on step output

Both are already specced but not yet implemented. Implementing them is sufficient to enable the routing patterns described in i-2, including LLM-based intent routing.

**One spec micro-extension is required**: §11 must explicitly include `pipeline:` paths in the set of fields where template variables resolve. Without this, `pipeline:` paths are static strings set at parse time, which forces enumeration of all targets at authoring time and prevents dynamic routing.

---

## Two Routing Patterns

### Pattern 1 — Static routing via `on_result`

Use when the set of target pipelines is known at authoring time. Explicit and auditable.

```yaml
- id: classify
  prompt: |
    Classify the user's request. Reply with exactly one word:
    PLANNING, IMPLEMENTATION, or DEBUGGING.
  on_result:
    - contains: "PLANNING"
      action: pipeline: ./pipelines/planning.yaml
    - contains: "IMPLEMENTATION"
      action: pipeline: ./pipelines/feature-dev.yaml
    - contains: "DEBUGGING"
      action: pipeline: ./pipelines/debugging.yaml
    - always: true
      action: abort_pipeline
```

### Pattern 2 — Dynamic routing via template variable

Use when the target pipeline is not known at authoring time and must be chosen at runtime. Requires §11 micro-extension.

```yaml
- id: selector
  prompt: |
    Based on the context, which pipeline should handle this next?
    Reply with only the relative path to the pipeline file, e.g. ./pipelines/planning.yaml

- id: delegate
  pipeline: "{{ step.selector.response }}"
```

Both patterns produce a sub-pipeline call: the selected pipeline runs in isolation, returns its output to the caller, and control returns to the next step (or the pipeline ends via `break`).

---

## §11 Spec Micro-Extension Required

**Current §11 text (line 1):**
> Prompt strings and file-based prompts may reference runtime context using `{{ variable }}` syntax.

**Updated text:**
> Prompt strings, file-based prompts, and `pipeline:` paths may reference runtime context using `{{ variable }}` syntax.

This one-sentence change unlocks Pattern 2. At execution time, before loading the sub-pipeline YAML, the runtime applies `template::resolve()` to the path string. If the variable is unresolved, the step aborts with `TEMPLATE_UNRESOLVED` — consistent with existing behavior for prompt steps.

---

## Why Not `switch_pipeline`

The i-2 document proposes a `switch_pipeline` directive that would replace the active session pipeline rather than calling a sub-pipeline. This introduces a different execution semantic:

**Sub-pipeline call (§9 model):** The caller remains on the stack. After the called pipeline completes, control returns to the caller's next step. Execution model: call stack.

**`switch_pipeline` (proposed):** The current pipeline is abandoned. The new pipeline becomes active. No return. Execution model: `goto`.

The `switch_pipeline` semantic creates problems that don't exist with sub-pipeline calls:

- **Session-state mutation.** `Session.pipeline` is treated as immutable for the duration of a run. Changing it mid-execution affects the executor, TUI sidebar, turn log, and any component holding a pipeline reference.
- **Audit trail discontinuity.** The turn log shows a pipeline that started but never completed, followed by a new pipeline that started without an invocation. The append-only NDJSON model doesn't represent this cleanly.
- **No advantage over sub-pipeline + break.** Any "I am the wrong pipeline, hand off" pattern is expressible as: call the right pipeline via `pipeline:` step, then `break`. The effect is identical but the execution semantics are clean.

---

## Constraint Statement

Pipeline switching at the session level is not a spec-level concern in v0.1 or v0.2. User-initiated pipeline switching is a TUI-layer concern (i-1). Pipeline-initiated delegation is expressed through the `pipeline:` step body and `on_result: pipeline: <path>` action (both already specced in §5 and §9), extended by the §11 micro-extension to allow template variables in `pipeline:` paths.

---

## Implementation Notes (v0.2)

When implementing `StepBody::SubPipeline` execution in `executor.rs`:

1. **Template-resolve the path first.** Before calling `config::load()`, apply `template::resolve()` to the path string (after converting `PathBuf` to `str`). If unresolved, abort with `TEMPLATE_UNRESOLVED`.
2. **Load and validate.** Call `config::load()` on the resolved path. If load fails, return `AilError` — not a panic.
3. **Execute in isolation.** Create a child `Session` using the caller's current `last_response` as the invocation prompt. Call `execute()` recursively.
4. **Capture output.** The child pipeline's final step response becomes the `TurnEntry.response` for the calling `pipeline:` step.
5. **Same pattern for `on_result: pipeline:`.** When evaluating `ResultAction::Pipeline(path)`, apply template resolution to `path` before loading.

Key files:
- `ail-core/src/executor.rs` — handle `StepBody::SubPipeline` (currently errors at line 233)
- `ail-core/src/config/domain.rs` — add `Pipeline(String)` variant to `ResultAction` (String, not PathBuf, to allow template resolution)
- `ail-core/src/config/validation.rs` — parse `pipeline:` action string in `on_result` branches
- `ail-core/src/config/dto.rs` — `OnResultBranchDto` may need a `pipeline` field

---

## Revisit Triggers

This decision should be revisited if:

1. **Fire-and-forget patterns emerge.** Users consistently need to hand off without returning — the sub-pipeline + break boilerplate becomes a pain point.
2. **Isolation model is too restrictive.** Called pipelines need to share session state (turn log, runner sessions) with the caller.
3. **D-019 implementation requires it.** The self-modifying pipeline reflection step needs to replace the active pipeline rather than layer a `FROM` modification.
4. **i-1 TUI patterns reveal the need.** Interactive session-level pipeline replacement would provide meaningfully better UX than the `:` picker combined with the current execution model.

---

## Success Criteria (from i-2)

- [x] Scenario decision is made (A)
- [x] Rationale for the decision is documented
- [x] Design constraint is clearly stated (no `switch_pipeline` directive)
- [x] §11 micro-extension identifies where spec change is needed
- [x] Both routing patterns (static and dynamic) are shown with examples
- [x] Runtime integration model specified (sub-pipeline isolation, template resolution)
- [x] Error handling strategy defined (TEMPLATE_UNRESOLVED on bad path, load errors propagate)
- [x] Trade-offs are clear and defensible
