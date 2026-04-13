## 27. `do_while:` — Bounded Repeat-Until

> **Implementation status:** Planned — v0.3 target. `do_while:` is a reserved primary field. The parser will reject it with `CONFIG_VALIDATION_FAILED` until implementation is complete.

A `do_while:` step runs a fixed set of inner steps repeatedly until an `exit_when` condition is met or a declared `max_iterations` bound is exceeded. It is the generate→test→fix pattern: each iteration produces a result, then checks whether the result is good enough to stop.

The condition is evaluated **after** each complete iteration — all inner steps run first, then `exit_when` is checked. This is semantically equivalent to a do-while loop in traditional programming.

---

### 27.1 Syntax

```yaml
- id: fix_loop
  do_while:
    max_iterations: 5
    exit_when: "{{ step.test.exit_code }} == 0"
    on_max_iterations: abort_pipeline
  steps:
    - id: fix
      prompt: |
        Iteration {{ do_while.iteration }} of {{ do_while.max_iterations }}.
        Fix the failing tests.
        Test output:
        {{ step.test.result }}
      resume: true
    - id: test
      context:
        shell: "cargo test 2>&1"
```

In this example: `test` runs first (iteration 1), then `exit_when` is evaluated against `step.test.exit_code`. If the tests pass (exit code 0), the loop exits. Otherwise, `fix` runs with the test output, followed by `test` again.

---

### 27.2 Fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `max_iterations` | Integer ≥ 1 | **Yes** | None | Hard upper bound on iterations. Author must declare — no unbounded loops. |
| `exit_when` | Condition expression (§12.2 syntax) | **Yes** | None | Evaluated after each complete iteration. When true, the loop exits cleanly. |
| `on_max_iterations` | `abort_pipeline` / `continue` / `pause_for_human` | No | `abort_pipeline` | What happens when the iteration budget is exhausted without `exit_when` becoming true. |
| `steps` | Array of Step | **Yes** | None | Inner steps executed each iteration. Same schema as top-level pipeline steps. |

`max_iterations` has no default value — the author must declare a bound. This is intentional: unbounded loops are a system-level failure mode that `ail` does not allow by omission.

---

### 27.3 Execution Semantics

1. **Post-iteration evaluation.** After all inner steps complete for an iteration, `exit_when` is evaluated. If true, the loop exits and the pipeline continues. If false, the next iteration begins (if budget remains).

2. **Primary field exclusivity.** `do_while:` is a primary field. A step cannot combine it with `prompt:`, `skill:`, `context:`, `pipeline:`, or `for_each:`.

3. **Step ID namespacing.** Inner step IDs are namespaced as `<loop_id>::<step_id>` (e.g. `fix_loop::test`). Within the loop body, the shorthand `step.<step_id>` resolves to the current iteration's result. From outside the loop, the qualified form `step.<loop_id>::<step_id>` is required.

4. **Iteration scope.** Only the current iteration's step results are in template variable scope. Prior iterations are recorded in the turn log but do not accumulate in working memory.

5. **`break` exits the loop, not the pipeline.** `break` in an `on_result` action inside a `do_while:` body exits the loop cleanly — the pipeline continues with the step after the loop. To exit the entire pipeline from within a loop, use `abort_pipeline`. See §5.4 for the full action reference.

6. **`before:` / `then:` on the loop step wrap the entire loop.** They run once — before the first iteration and after the loop completes respectively. `before:` and `then:` declared on inner steps run per-iteration.

7. **Inner step `on_error` works per-step.** The `do_while:` step itself can declare `on_error` for infrastructure failures (runner crash, timeout). Inner step failures are handled by each inner step's own `on_error`.

8. **`on_result` on the loop step fires after the loop completes.** The "result" is the last iteration's last inner step result. Use this to branch based on whether the loop exited via `exit_when` vs `max_iterations`.

---

### 27.4 Template Variables Inside `do_while:` Bodies

These variables are available only within the `steps:` block of a `do_while:` step:

| Variable | Value |
|---|---|
| `{{ do_while.iteration }}` | Current 1-based iteration number |
| `{{ do_while.max_iterations }}` | The declared `max_iterations` value |
| `{{ step.<step_id>.response }}` | Shorthand — resolves the current iteration's result for inner step `<step_id>` |
| `{{ step.<loop_id>::<step_id>.response }}` | Qualified form — also works from outside the loop |

`{{ do_while.iteration }}` and `{{ do_while.max_iterations }}` are not available outside a `do_while:` body. Referencing them at the top level is a `TEMPLATE_UNRESOLVED` error.

---

### 27.5 `exit_when` Expression Language

`exit_when` uses the same condition expression syntax as `condition:` (§12.2):

```yaml
# Check exit code of a shell step
exit_when: "{{ step.test.exit_code }} == 0"

# Check if a prompt step response contains a keyword
exit_when: "{{ step.review.response }} contains 'LGTM'"

# Check stdout of a context step
exit_when: "{{ step.lint.stdout }} contains 'no warnings'"
```

See §12.2 for the full operator list (`==`, `!=`, `contains`, `starts_with`, `ends_with`).

`exit_when` is validated as a valid condition expression at parse time. An unrecognised expression is a `CONFIG_VALIDATION_FAILED` error.

---

### 27.6 Turn Log Events

Each loop produces the following NDJSON events in the pipeline run log (§4.4):

```json
{"type": "do_while_started", "step_id": "fix_loop", "max_iterations": 5}
{"type": "do_while_iteration_started", "step_id": "fix_loop", "iteration": 1}
{"type": "step_started", "step_id": "fix_loop::fix", ...}
{"type": "step_completed", "step_id": "fix_loop::fix", ...}
{"type": "step_started", "step_id": "fix_loop::test", ...}
{"type": "step_completed", "step_id": "fix_loop::test", ...}
{"type": "do_while_exit_when_evaluated", "step_id": "fix_loop", "iteration": 1, "result": false}
{"type": "do_while_iteration_started", "step_id": "fix_loop", "iteration": 2}
...
{"type": "do_while_completed", "step_id": "fix_loop", "iterations_used": 3, "exit_reason": "exit_when"}
```

**Exit reasons:**

| Value | Meaning |
|---|---|
| `"exit_when"` | `exit_when` evaluated to true |
| `"break"` | A `break` action fired inside the loop body |
| `"max_iterations"` | Iteration budget exhausted |

---

### 27.7 Executor Events (Controlled Mode)

For pipelines running in `--output-format json` controlled mode (§4.5), the following additional executor events are emitted:

| Event | When |
|---|---|
| `DoWhileStarted { step_id, max_iterations }` | Before the first iteration begins |
| `DoWhileIterationStarted { step_id, iteration }` | Before each iteration |
| `DoWhileCompleted { step_id, iterations_used, exit_reason }` | After the loop exits |
| `DoWhileMaxIterationsExceeded { step_id, max_iterations }` | When the iteration budget fires (before `on_max_iterations` action) |

---

### 27.8 Validation Rules

1. `do_while:` requires `max_iterations`, `exit_when`, and `steps` (non-empty). Any missing → `CONFIG_VALIDATION_FAILED`.
2. `do_while:` is mutually exclusive with all other primary fields (`prompt:`, `skill:`, `context:`, `pipeline:`, `for_each:`). Combining them → `CONFIG_VALIDATION_FAILED`.
3. `exit_when` is validated as a valid condition expression (§12.2) at parse time.
4. `max_iterations` must be an integer ≥ 1.
5. Step IDs within `steps` must be unique within the loop (they may reuse IDs that appear outside the loop — the `<loop_id>::` prefix disambiguates them).
6. `on_max_iterations` must be one of `abort_pipeline`, `continue`, `pause_for_human`. Unknown value → `CONFIG_VALIDATION_FAILED`.

---

### 27.9 Nesting

`do_while:` steps may be nested. Inner and outer loops are fully independent. The runtime enforces a hard depth limit to prevent infinite recursion from misconfigured pipelines. When the depth limit is exceeded, the pipeline aborts with `LOOP_DEPTH_EXCEEDED` (`ail:loop/depth-exceeded`). The depth limit is determined at implementation time and is not configurable.

---

### 27.10 New Error Types

| Constant | Value | When produced |
|---|---|---|
| `DO_WHILE_MAX_ITERATIONS` | `ail:do-while/max-iterations-exceeded` | Loop hit `max_iterations` with `on_max_iterations: abort_pipeline` |
| `LOOP_DEPTH_EXCEEDED` | `ail:loop/depth-exceeded` | Nested loops exceeded the runtime depth limit |

---
