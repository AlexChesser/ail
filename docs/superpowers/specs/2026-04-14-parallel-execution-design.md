---
name: Parallel Step Execution Design
description: Design decisions for issue #117 — async:, depends_on:, action: join, session fork model, structured join
type: project
date: 2026-04-14
issue: "#117"
spec: spec/core/s29-parallel-execution.md
---

# Parallel Step Execution — Design Document

**Issue:** #117
**Spec:** `spec/core/s29-parallel-execution.md`
**Date:** 2026-04-14

## Summary

Parallel step execution allows independent pipeline steps to run concurrently using a DAG model. Two new step fields (`async:`, `depends_on:`) and one new action value (`action: join`) form the complete primitive set. The design favors explicit declaration over implicit inference — nothing runs in parallel unless explicitly marked.

---

## Design Decisions

### D-1: Syntax model — DAG with `async:` + `depends_on:`

**Decision:** DAG model using `async: true` on individual steps and `depends_on: [step_ids]` for explicit join barriers. Rejected the `parallel:` grouped block (requires a parent wrapper step) and the `group:` tag model (flat but less readable).

**Rationale:** `depends_on` has unlimited potential — it scales cleanly from simple two-step parallelism to complex multi-branch DAGs without nesting. It is also the conventional model in CI systems (GitHub Actions, Buildkite) that pipeline authors already understand.

### D-2: Orphaned async steps — validation error

**Decision:** Every `async: true` step must be named in at least one `depends_on` list. Orphaned async steps are a **parse-time validation error**.

**Rationale:** Silent fire-and-forget is unauditable. If the user genuinely wants to collect without acting, `action: join` with no `on_result` serves that purpose. The pipeline graph must be fully declared and checkable at parse time.

### D-3: `action: join` is not a no-op

**Decision:** `action: join` is a synchronization and merge step — it concatenates dependency responses, makes `on_result` available, and produces `{{ step.<id>.response }}` for downstream steps.

**Rationale:** Even when no LLM synthesis is needed, `on_result` on a join is meaningful — `contains: "FAIL"` across the merged output catches failures from any branch. A true no-op would lose this capability.

### D-4: Structured join via `output_schema` namespacing

**Decision:** When all `depends_on` steps declare `output_schema`, the join automatically namespaces their structured outputs under their step IDs into a single merged JSON object. The join step may declare `output_schema` to validate the merged shape. `on_result` with `field:` + `equals:` can then branch on dotted paths (`field: lint.clean`).

**Rationale:** Reuses existing `field:` operator path syntax and `output_schema` machinery from §26. No new template machinery required. Mixed structured/unstructured dependencies on a structured join are a validation error — forces consistency.

**Fallback:** if any dependency lacks `output_schema`, the join falls back to string concatenation.

### D-5: Session fork model

**Decision:** When an `async` step launches, the executor forks the session context at the current sequential point — new independent runner session, seeded with all prior sequential conversation history. Concurrent async steps are invisible to each other. `resume: false` opts out of context inheritance for a clean isolated session.

**Rationale:** "Starts from what was known before it launched" is the intuitive mental model. Concurrent async steps sharing a session is a race condition; the spec forbids it (parse-time error if `resume: true` on concurrent steps targeting the same session).

**Runner note:** HTTP runner forks cleanly (copy messages array). Claude CLI runner re-injects prior context best-effort. The spec mandates the intent; compliance tiers govern the guarantee.

### D-6: Error handling — configurable `on_error` on join, default `fail_fast`

**Decision:** `on_error: fail_fast | wait_for_all` declared on the join step. Default is `fail_fast`. `fail_fast` sends active cancel signals to all other in-flight branches.

**Rationale:** `fail_fast` is the right default — in most pipelines (CI, review gates), a lint failure makes continuing the test run wasteful. `wait_for_all` is available for reporting pipelines where partial results are valuable. Active cancellation (not just ignoring) keeps the turn log complete and prevents resource waste.

### D-7: Turn log `concurrent_group` + `launched_at`/`completed_at`

**Decision:** New `concurrent_group` field (shared ID for steps in-flight simultaneously) and explicit `launched_at`/`completed_at` timestamps on all entries. Cancel events get a dedicated `step_cancelled` entry type.

**Rationale:** Async steps complete out of order. Wall-clock timestamps are the only reliable ordering for reconstruction. `concurrent_group` lets `ail log` reconstruct and display the parallel execution timeline.

### D-8: Resource limits via `defaults.max_concurrency`

**Decision:** Pipeline-wide cap via `defaults.max_concurrency`. No per-join limit. Default is unlimited.

**Rationale:** Per-join concurrency limits are awkward because async steps are declared independently of their collectors. A global cap is simpler and covers the practical need (preventing runaway parallelism on constrained infrastructure).

---

## Validation Rules Summary

| Rule | Timing |
|---|---|
| `async: true` step not named in any `depends_on` | Parse error |
| `action: join` with empty/missing `depends_on` | Parse error |
| `depends_on` forward reference | Parse error |
| `depends_on` circular chain | Parse error |
| `{{ step.<async_id>.* }}` without dependency path | Parse error |
| `resume: true` on concurrent steps sharing a session | Parse error |
| Structured join (`output_schema`) with unstructured dependency | Parse error |
| Join `on_result` uses `field:` + `equals:` without `output_schema` | Parse error |

---

## Template Variables Added

| Variable | Resolves to |
|---|---|
| `{{ step.<join_id>.response }}` | Concatenated string output of a string join |
| `{{ step.<join_id>.<dep_id>.response }}` | Full structured output of a named dependency in a structured join |
| `{{ step.<join_id>.<dep_id>.<field> }}` | Specific field within a namespaced structured dependency output |

---

## Open Questions (deferred)

- **Async steps inside `do_while:` / `for_each:` bodies:** cross-iteration dependencies are explicitly forbidden (validation error). Within-iteration parallelism is supported. Full semantics should be tested during implementation.
- **`before:` / `then:` chain inheritance of `async:`:** specified as inheriting the parent's async flag unless overridden. May need refinement during implementation.
- **Cancel signal reliability:** best-effort by spec. A future runner compliance tier could make graceful cancellation a hard requirement.
- **`ail log` timeline display:** the turn log format supports reconstruction; the display format for parallel timelines in `ail log` is not yet designed.
