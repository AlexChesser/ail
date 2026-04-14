## 28. `for_each:` — Collection Iteration

> **Implementation status:** Fully implemented. `for_each:` — parse-time validation (`over`, `as`, `max_items`, `on_max_items`, `steps`/`pipeline`), runtime array iteration with item scope, `{{ for_each.item }}` / `{{ for_each.<as_name> }}` / `{{ for_each.index }}` / `{{ for_each.total }}` template variables, `break` exits loop not pipeline, `max_items` cap with `on_max_items` behavior, step ID namespacing (`<loop_id>::<step_id>`), shared depth guard with `do_while:`. `pipeline:` as alternative to inline `steps:` implemented. Controlled-mode executor events (§28.6) are deferred.

A `for_each:` step runs a fixed set of inner steps once per item in a validated array produced by a prior step. Where `do_while:` (§27) repeats until a condition is met, `for_each:` maps a sub-pipeline across a known collection — the plan-execution pattern: generate a list of tasks, then implement each one.

**Prerequisite:** The source step must declare `output_schema` (§26) with `type: array`. `for_each:` requires a validated, typed array — not a string that happens to look like a list. This constraint exists to prevent silent failures from non-deterministic LLM list formatting.

---

### 28.1 Syntax

```yaml
- id: plan
  prompt: "Break this feature into implementation tasks. Respond with a JSON array of strings."
  output_schema:
    type: array
    items:
      type: string
    maxItems: 20

- id: implement_tasks
  for_each:
    over: "{{ step.plan.items }}"
    as: task
    max_items: 20
    on_max_items: abort_pipeline
    steps:
      - id: implement
        prompt: |
          Task {{ for_each.index }} of {{ for_each.total }}: {{ for_each.task }}
          Implement this task completely.
        resume: true
      - id: verify
        context:
          shell: "cargo test 2>&1"
```

The loop body can also reference an external pipeline file instead of inline steps:

```yaml
- id: implement_tasks
  for_each:
    over: "{{ step.plan.items }}"
    as: task
    pipeline: ./task-handler.ail.yaml
```

The referenced file's `pipeline:` steps become the loop body. Template variables (`{{ for_each.item }}`, `{{ for_each.index }}`, etc.) are available inside the referenced pipeline.

---

### 28.2 Fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `over` | Template expression resolving to a validated array | **Yes** | None | Must reference `{{ step.<id>.items }}` where step `<id>` declared `output_schema` with `type: array`. |
| `as` | Identifier string | No | `item` | Local name for the current item within the loop body. Accessed as `{{ for_each.<as> }}`. |
| `max_items` | Integer ≥ 1 | No | None | Hard cap on items processed. Items beyond this limit are not processed. |
| `on_max_items` | `abort_pipeline` / `continue` | No | `continue` | What happens when the array contains more items than `max_items`. `continue` silently skips excess items. `abort_pipeline` treats excess items as a fatal error. |
| `steps` | Array of Step | Conditional | None | Inner steps executed once per item. Same schema as top-level pipeline steps. Required if `pipeline` is not set. |
| `pipeline` | String (file path) | Conditional | None | Path to an external `.ail.yaml` file whose `pipeline:` steps become the loop body. Resolved relative to the current pipeline file at parse time. Required if `steps` is not set. |

`steps` and `pipeline` are mutually exclusive — declare one or the other. Declaring both or neither is a `CONFIG_VALIDATION_FAILED` error.

---

### 28.3 Execution Semantics

1. **Items resolved once at start.** The array is fetched from the source step's validated output before the first item is processed. The array is immutable during `for_each:` execution — mid-loop modifications to the source step's output (if any) do not affect the running iteration.

2. **Primary field exclusivity.** `for_each:` is a primary field. A step cannot combine it with `prompt:`, `skill:`, `context:`, `pipeline:`, or `do_while:`.

3. **Step ID namespacing.** Inner step IDs are namespaced as `<loop_id>::<step_id>` (e.g. `implement_tasks::implement`). Within the loop body, the shorthand `step.<step_id>` resolves to the current item's result. From outside the loop, the qualified form `step.<loop_id>::<step_id>` is required.

4. **Item scope.** Only the current item's step results are in template variable scope. Prior items are recorded in the turn log but do not accumulate in working memory.

5. **`break` exits the loop, not the pipeline.** `break` in an `on_result` action inside a `for_each:` body exits the loop cleanly — the pipeline continues with the step after the loop. To exit the entire pipeline, use `abort_pipeline`. See §5.4 for the full action reference.

6. **`before:` / `then:` on the loop step wrap the entire loop.** They run once. `before:` and `then:` on inner steps run per-item.

7. **Inner step `on_error` works per-step.** The `for_each:` step itself can declare `on_error` for infrastructure failures. Inner step failures are handled by each inner step's own `on_error`.

8. **`on_result` on the loop step fires after all items are processed.** The "result" is the last item's last inner step result.

---

### 28.4 Template Variables Inside `for_each:` Bodies

These variables are available only within the `steps:` block of a `for_each:` step:

| Variable | Value |
|---|---|
| `{{ for_each.item }}` | Current item value. Default name — used when `as:` is not set. |
| `{{ for_each.<as_name> }}` | Current item value under the declared `as:` name (e.g. `{{ for_each.task }}` when `as: task`). |
| `{{ for_each.index }}` | Current 1-based item index. |
| `{{ for_each.total }}` | Total number of items in the collection (after `max_items` cap is applied). |

`for_each.*` variables are not available outside a `for_each:` body. Referencing them at the top level is a `TEMPLATE_UNRESOLVED` error.

---

### 28.5 Turn Log Events

Each `for_each:` loop produces the following NDJSON events in the pipeline run log (§4.4):

```json
{"type": "for_each_started", "step_id": "implement_tasks", "total_items": 7}
{"type": "for_each_item_started", "step_id": "implement_tasks", "index": 1, "item": "Add user authentication"}
{"type": "step_started", "step_id": "implement_tasks::implement", ...}
{"type": "step_completed", "step_id": "implement_tasks::implement", ...}
{"type": "step_started", "step_id": "implement_tasks::verify", ...}
{"type": "step_completed", "step_id": "implement_tasks::verify", ...}
{"type": "for_each_item_completed", "step_id": "implement_tasks", "index": 1}
{"type": "for_each_item_started", "step_id": "implement_tasks", "index": 2, "item": "Add password reset flow"}
...
{"type": "for_each_completed", "step_id": "implement_tasks", "items_processed": 7, "exit_reason": "completed"}
```

**Exit reasons:**

| Value | Meaning |
|---|---|
| `"completed"` | All items processed |
| `"break"` | A `break` action fired inside the loop body |

---

### 28.6 Executor Events (Controlled Mode)

For pipelines running in `--output-format json` controlled mode (§4.5):

| Event | When |
|---|---|
| `ForEachStarted { step_id, total_items }` | Before the first item is processed |
| `ForEachItemStarted { step_id, index, item }` | Before each item |
| `ForEachItemCompleted { step_id, index }` | After each item's inner steps complete |
| `ForEachCompleted { step_id, items_processed, exit_reason }` | After the loop finishes |

---

### 28.7 Validation Rules

1. `for_each:` requires `over` and either `steps` (non-empty) or `pipeline`. Missing both or providing both → `CONFIG_VALIDATION_FAILED`.
2. `over` must reference `{{ step.<id>.items }}` where step `<id>` declared `output_schema` with `type: array`. No schema, or non-array schema → `FOR_EACH_SOURCE_INVALID`.
3. `for_each:` is mutually exclusive with all other primary fields. Combining them → `CONFIG_VALIDATION_FAILED`.
4. `as` must be a valid identifier (letters, digits, underscores; does not start with a digit). Invalid → `CONFIG_VALIDATION_FAILED`.
5. `max_items` must be an integer ≥ 1 if specified.
6. `on_max_items` must be one of `abort_pipeline`, `continue`. Unknown value → `CONFIG_VALIDATION_FAILED`.
7. Step IDs within `steps` must be unique within the loop.
8. `pipeline` file path must be resolvable and the referenced file must be a valid pipeline. File not found → `CONFIG_FILE_NOT_FOUND`.

---

### 28.8 New Error Types

| Constant | Value | When produced |
|---|---|---|
| `FOR_EACH_SOURCE_INVALID` | `ail:for-each/source-invalid` | `for_each.over` references a step that did not declare `output_schema: type: array` |

---
