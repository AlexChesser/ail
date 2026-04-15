## 29. Parallel Step Execution

> **Implementation status:** Implemented in v0.3 (issue #117). Uses `std::thread::scope` for
> scoped-thread dispatch. `do_while[N]` indexed access (§29.12 intersection with §27.4) and
> controlled-mode executor events for async launches are deferred.

Independent pipeline steps running sequentially is pure waste — lint and test do not depend on each other. Parallel step execution allows independent steps to run concurrently, with explicit synchronization points that merge results and gate further execution. This also unlocks multi-provider comparison patterns and fan-out/fan-in workflows.

---

### 29.1 New step-level fields

Two new fields are added to the step specification:

#### `async: true`

Marks a step as non-blocking. The pipeline cursor advances to the next declared step immediately after launching this one. The async step runs concurrently with subsequent sequential steps and with other async steps.

An async step that is not named in any `depends_on` list is a **parse-time validation error** — every async step must have at least one collector.

#### `depends_on: [step_id, ...]`

Declares that this step must not begin executing until all named steps have completed. Valid on any step type. A step with both `async: true` and `depends_on:` launches immediately (non-blocking to the sequential cursor) but waits for its declared dependencies to complete before its own body executes.

`depends_on` references must name steps declared earlier in the pipeline. Forward references and self-references are validation errors. Circular chains are a validation error (cycle detection at parse time).

---

### 29.2 Basic example

```yaml
- id: lint
  async: true
  prompt: "Run lint and report any issues."

- id: test
  async: true
  prompt: "Run the full test suite."

- id: prepare_release_notes   # runs while lint and test are in-flight
  prompt: "Draft the release notes for this change."

- id: checks_done
  depends_on: [lint, test]
  action: join
  on_result:
    - contains: "FAIL"
      action: abort_pipeline
    - always:
      action: continue

- id: ship
  prompt: "All checks passed. Finalize the release."
```

Execution timeline:
```
lint (async)   ──────────────────────────┐
test (async)   ─────────────────────────────────┐
prepare_notes  ──────────┤                       │
                          │                      │
checks_done    ───────────┴──────────────────────┤ (join barrier)
ship           ──────────────────────────────────┘
```

---

### 29.3 `action: join`

`action: join` is a synchronization step. It:

1. Waits for all `depends_on` steps to complete
2. Merges their outputs (see §29.4 and §29.5)
3. Evaluates `on_result` against the merged output
4. Makes the result available as `{{ step.<id>.response }}` to subsequent steps

`action: join` makes no LLM call and has no runner session of its own.

`action: join` requires a non-empty `depends_on` list — a join with no dependencies is a validation error.

`on_result` on a join step is fully supported. It evaluates against the merged output and may use any match operator, including `field:` + `equals:` when the join declares `output_schema` (see §29.5).

---

### 29.4 String join — default merge behavior

When the `depends_on` steps do not all declare `output_schema`, the join concatenates their responses in declaration order, each prefixed with a labelled header:

```
[lint]:
No issues found.

[test]:
All 42 tests passed.
```

This concatenated string is the join step's response, accessible as `{{ step.<join_id>.response }}`.

---

### 29.5 Structured join — `output_schema` merge

When **all** `depends_on` steps declare `output_schema`, the join automatically namespaces their structured outputs under their step IDs into a single merged JSON object:

```json
{
  "lint": { "clean": true, "issues": [] },
  "test": { "passed": 42, "failed": 0, "clean": true }
}
```

The join step may declare `output_schema` to validate this merged shape:

```yaml
- id: lint
  async: true
  prompt: "Run lint. Respond with JSON only."
  output_schema:
    type: object
    properties:
      clean: { type: boolean }
      issues: { type: array, items: { type: string } }
    required: [clean]

- id: test
  async: true
  prompt: "Run tests. Respond with JSON only."
  output_schema:
    type: object
    properties:
      passed: { type: integer }
      failed: { type: integer }
      clean: { type: boolean }
    required: [clean]

- id: checks_done
  depends_on: [lint, test]
  action: join
  output_schema:
    type: object
    properties:
      lint:
        type: object
        properties:
          clean: { type: boolean }
        required: [clean]
      test:
        type: object
        properties:
          clean: { type: boolean }
        required: [clean]
    required: [lint, test]
  on_result:
    - field: lint.clean
      equals: false
      action: abort_pipeline
    - field: test.clean
      equals: false
      action: abort_pipeline
    - always:
      action: continue
```

**Parse-time compatibility:** if the join declares `output_schema` and all `depends_on` steps declare `output_schema`, the runtime validates at parse time that each dependency's declared output shape is compatible with the corresponding key in the join's `output_schema`.

**Mixing structured and unstructured:** if the join declares `output_schema` but one or more `depends_on` steps do not declare `output_schema`, this is a **parse-time validation error**. All dependencies must be structured if the join is structured.

**Error envelopes in `wait_for_all` mode:** when `on_error: wait_for_all` (§29.7), a dependency that fails contributes an error envelope under its key rather than its structured output:

```json
{
  "lint": { "clean": true, "issues": [] },
  "test": { "error": "Runner exited with code 1", "error_type": "RUNNER_ERROR" }
}
```

The join's `output_schema` should account for this possibility, or `on_result` should branch before strict field access.

---

### 29.6 Template variable scoping

Async step outputs are only accessible to steps that have a declared dependency path to them. This is enforced at **parse time**.

A step may reference `{{ step.<id>.response }}` for an async step only if:

- It declares `depends_on: [<id>]` directly, or
- It follows a `join` step whose `depends_on` list includes `<id>`

Referencing an async step's output without a declared dependency path is a **parse-time validation error**.

**New template variables introduced by parallel execution:**

| Variable | Resolves to |
|---|---|
| `{{ step.<join_id>.response }}` | Concatenated string output of a string join step |
| `{{ step.<join_id>.<dep_id>.response }}` | Namespaced structured field from a structured join — the full output of dependency `<dep_id>` |
| `{{ step.<join_id>.<dep_id>.<field> }}` | Specific field within a namespaced structured dependency output |

The dotted-path accessor on join outputs reuses the existing `field:` operator path syntax — no new template machinery is required.

---

### 29.7 Error handling and cancellation

`on_error` is declared on the `join` step and governs behavior when a dependency fails:

#### `fail_fast` (default)

The first dependency failure sends a **cancel signal** to all other in-flight branches, then surfaces the error at the join. `on_result` does not fire — the error propagates via `on_error` (§16). The join step's turn log entry records which step triggered the failure and which branches received cancel signals.

```yaml
- id: checks_done
  depends_on: [lint, test, typecheck]
  action: join
  # on_error: fail_fast  ← default; may be omitted
```

#### `wait_for_all`

All branches run to completion regardless of individual failures. Failed branches contribute error envelopes to the merged output. `on_result` fires against the full merged result — the pipeline can inspect which branches failed and decide accordingly.

```yaml
- id: checks_done
  depends_on: [lint, test, typecheck]
  action: join
  on_error: wait_for_all
  on_result:
    - contains: "RUNNER_ERROR"
      action: pause_for_human
      message: "One or more checks failed. Review before proceeding."
    - always:
      action: continue
```

**Cancel signals** are best-effort. The runtime attempts to cancel in-flight branches; whether graceful cancellation is achievable depends on runner compliance. The turn log records the cancel signal and its outcome regardless (see §29.8).

---

### 29.8 Turn log format for concurrent execution

Turn log entries gain two fields when steps run concurrently:

**`concurrent_group`** — a generated ID shared by all steps that were in-flight simultaneously. Steps launched from the same sequential point share a group ID. Sequential steps have `concurrent_group: null`.

**`launched_at` / `completed_at`** — explicit wall-clock timestamps on every entry. Async steps may complete out of order; timestamps are the authoritative ordering for reconstruction.

```jsonl
{"step_id": "lint", "launched_at": "2026-04-14T10:00:00.100Z", "completed_at": "2026-04-14T10:00:03.210Z", "concurrent_group": "cg-a1b2", "session_id": "s_lint", ...}
{"step_id": "test", "launched_at": "2026-04-14T10:00:00.110Z", "completed_at": "2026-04-14T10:00:08.540Z", "concurrent_group": "cg-a1b2", "session_id": "s_test", ...}
{"step_id": "prepare_release_notes", "launched_at": "2026-04-14T10:00:00.115Z", "completed_at": "2026-04-14T10:00:01.800Z", "concurrent_group": null, "session_id": "s_main", ...}
{"step_id": "checks_done", "launched_at": "2026-04-14T10:00:08.541Z", "completed_at": "2026-04-14T10:00:08.545Z", "concurrent_group": null, "session_id": null, "action": "join", ...}
```

Cancel events are recorded as a separate entry type:

```jsonl
{"event": "step_cancelled", "step_id": "test", "cancelled_by": "lint", "reason": "fail_fast", "at": "2026-04-14T10:00:03.215Z"}
```

The `ail log` command (§24) uses `concurrent_group` and timestamps to reconstruct and display the parallel execution timeline.

---

### 29.9 Session fork model

When an `async` step launches, the executor **forks** the session context at the current sequential point:

- A new independent runner session is created for the async step
- All conversation history from prior sequential steps up to the launch point is injected as context — the async step starts from the same conversational state as the main sequential flow at that moment
- Multiple async steps launching from the same point all fork from the same pre-launch state — they are parallel branches of the same history, invisible to each other
- `resume: false` on an async step opts out of context inheritance entirely — the step gets a clean session with no prior history

```yaml
- id: plan
  prompt: "Outline the implementation plan."

- id: write_frontend
  async: true                   # forks from conversation after plan
  prompt: "Implement the frontend."

- id: write_backend
  async: true                   # forks from conversation after plan, independent of write_frontend
  prompt: "Implement the backend."

- id: security_scan
  async: true
  resume: false                 # clean isolated session — no context injected
  prompt: "Scan the codebase for vulnerabilities."

- id: integrate
  depends_on: [write_frontend, write_backend, security_scan]
  action: join
```

**Runner-specific implementation:**

| Runner | Fork mechanism |
|---|---|
| HTTP runner | Messages array copied to a new context at launch time — clean fork |
| Claude CLI runner | Prior context re-injected via conversation history replay at session start — best-effort |

The spec mandates the intent (async steps inherit pre-launch context); runner compliance tiers govern the guarantee.

**Concurrent session constraint:** two `async` steps that are in-flight simultaneously cannot share the same runner session. `resume: true` on a step that would run concurrently with another step targeting the same session is a **parse-time validation error**.

After a `join`, the merged result enters the main sequential flow. Subsequent steps see the prior sequential history plus the join's merged output.

---

### 29.10 Resource limits

A `max_concurrency` field under `defaults:` caps the number of simultaneously in-flight async steps across the entire pipeline. When the cap is reached, newly launched async steps queue in declaration order until a slot opens:

```yaml
defaults:
  max_concurrency: 4   # default: unlimited
```

There is no per-join concurrency limit — the cap is pipeline-wide.

---

### 29.11 Validation rules summary

| Rule | Error type |
|---|---|
| `async: true` step not named in any `depends_on` list | Parse error — orphaned async step |
| `action: join` with empty or missing `depends_on` | Parse error |
| `depends_on` referencing a step not yet declared | Parse error — forward reference |
| `depends_on` circular chain | Parse error — cycle detected |
| `{{ step.<async_id>.* }}` without a dependency path | Parse error — unresolvable reference |
| `resume: true` on a step concurrent with another sharing that session | Parse error |
| Join declares `output_schema` but a dependency does not | Parse error — mixed structured/unstructured join |
| Join `on_result` uses `field:` + `equals:` without `output_schema` | Parse error — same rule as §26.4 |

---

### 29.12 Interaction with existing features

**`condition:`** — evaluated before the step launches. A conditional async step that does not launch is treated as if it completed successfully for dependency resolution purposes — dependents that `depends_on` only that step become unblocked immediately.

**`on_result:` on async steps** — fully supported. Evaluated when the async step completes, before the join collects it. An `abort_pipeline` action on an async step's `on_result` cancels all in-flight steps and aborts immediately, regardless of the join's `on_error` policy.

**`before:` / `then:` step chains (§5.7, §5.10)** — compatible with `async:`. The `before:` and `then:` steps inherit the `async` flag of their parent unless overridden.

**`do_while:` and `for_each:` loops** — async steps may be declared inside loop bodies. `depends_on` references inside a loop body are scoped to the current iteration — cross-iteration and cross-loop dependencies are not supported and are a validation error.

**`pipeline:` sub-pipeline steps** — an async step may call a sub-pipeline. The sub-pipeline executes in full before the async step is considered complete.

---

### 29.13 Complete example — multi-check CI pipeline

```yaml
version: "0.1"

defaults:
  max_concurrency: 4

pipeline:
  - id: setup
    prompt: "Review the changed files and prepare a summary for the checks below."

  - id: lint
    async: true
    prompt: "Run lint on the changed files. Respond with JSON only."
    output_schema:
      type: object
      properties:
        clean: { type: boolean }
        issues: { type: array, items: { type: string } }
      required: [clean]

  - id: test
    async: true
    prompt: "Run the test suite. Respond with JSON only."
    output_schema:
      type: object
      properties:
        passed: { type: integer }
        failed: { type: integer }
        clean: { type: boolean }
      required: [clean]

  - id: security
    async: true
    resume: false
    prompt: "Run a security audit. Respond with JSON only."
    output_schema:
      type: object
      properties:
        clean: { type: boolean }
        findings: { type: array, items: { type: string } }
      required: [clean]

  - id: checks_done
    depends_on: [lint, test, security]
    action: join
    on_error: wait_for_all
    output_schema:
      type: object
      properties:
        lint:
          type: object
          properties:
            clean: { type: boolean }
          required: [clean]
        test:
          type: object
          properties:
            clean: { type: boolean }
          required: [clean]
        security:
          type: object
          properties:
            clean: { type: boolean }
          required: [clean]
      required: [lint, test, security]
    on_result:
      - field: security.clean
        equals: false
        action: pause_for_human
        message: "Security findings require review before merging."
      - field: lint.clean
        equals: false
        action: abort_pipeline
      - field: test.clean
        equals: false
        action: abort_pipeline
      - always:
        action: continue

  - id: ship
    prompt: |
      All checks passed:
      {{ step.checks_done.response }}
      Finalize the release notes and mark the PR ready for merge.
```
