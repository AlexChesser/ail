# Proposed GitHub Issues

These issues should be filed in `alexchesser/ail` to track spec gaps discovered during the superpowers-as-pipelines feasibility analysis. No GitHub CLI or MCP tools were available at authoring time.

---

## Issue 1: Looping / Iteration Construct for Pipeline Steps

**Title:** Spec proposal: looping / iteration construct (`iterate:` / `for_each:`)

**Labels:** enhancement, spec

**Body:**

### Problem

AIL pipelines are static YAML — the number and identity of steps is fixed at parse time. There is no way to express "for each task in a dynamically-generated list, execute a sub-pipeline." This blocks several important workflow patterns:

- **Executing plans** — A planning step produces N tasks; each task needs implementation + verification + commit. Without looping, the pipeline must handle all tasks in a single batch prompt, losing the error isolation and auditability benefits of per-task execution.

- **Subagent-driven development** — The [obra/superpowers](https://github.com/obra/superpowers) pattern dispatches a fresh subagent per task with two-stage review (spec compliance + code quality). This requires iterating over tasks and dispatching a sub-pipeline for each.

- **Review-fix cycles** — A reviewer identifies N issues; each needs a fix + re-review. Without looping, the pipeline can't model this iterative convergence.

### Proposed Syntax

```yaml
- id: per_task_execution
  iterate:
    over: "{{ step.plan.response | parse_tasks }}"  # or a structured output array
    as: task
  pipeline: ./implement-task.ail.yaml
  prompt: "{{ task }}"
```

Or a simpler variant that iterates over lines/items in a step response:

```yaml
- id: per_task_execution
  for_each:
    items: "{{ step.plan.response }}"
    separator: "\n---\n"       # how to split items
  pipeline: ./implement-task.ail.yaml
```

### Design Considerations

1. **Termination** — Must have a max iteration count (like `MAX_SUB_PIPELINE_DEPTH = 16`) to prevent infinite loops
2. **Template variable scoping** — How does the current iteration item become available as a template variable?
3. **Error semantics** — If one iteration fails, does the loop abort or continue? (Probably needs `on_error` per iteration)
4. **Structured I/O dependency** — `parse_tasks` implies structured output from the planning step. This may depend on `output_schema` (SPEC S21) being implemented first.
5. **Turn log** — Each iteration should produce its own `TurnEntry` for auditability

### Context

This gap was discovered while reproducing [obra/superpowers](https://github.com/obra/superpowers) as AIL pipeline workflows. See `demo/superpowers/executing-plans.ail.yaml` and `demo/superpowers/subagent-development.ail.yaml` for the workarounds (single-batch execution) and PROPOSED comments showing intended syntax.

---

## Issue 2: Parallel Step Execution — Design Completion

**Title:** Complete the parallel step execution design (SPEC S21)

**Labels:** enhancement, spec

**Body:**

### Problem

SPEC S21 lists parallel step execution as a planned extension but the design is incomplete. Several high-value workflow patterns are blocked:

- **Dispatching parallel agents** — The [obra/superpowers](https://github.com/obra/superpowers) pattern dispatches one agent per independent problem domain for concurrent investigation. Without `parallel:`, these must run sequentially, losing the primary benefit (time savings).

- **Multi-provider sampling** — Running the same prompt against multiple models concurrently for quality comparison.

- **Fan-out / fan-in** — Splitting work across parallel branches and synthesizing results.

### Current Spec Status

SPEC S21 mentions:
- `parallel:` block with concurrent child steps
- Fan-out / fan-in with `synthesize:` step
- Multi-provider parallel sampling

But the design doesn't cover:
- Failure semantics (one branch fails — do others continue?)
- Resource isolation (do parallel branches share session state?)
- Result merging (how does `synthesize:` access parallel branch outputs?)
- Template variable scoping for parallel branches
- Turn log ordering for concurrent steps

### Proposed Syntax

```yaml
- id: parallel_investigation
  parallel:
    - id: domain_1
      pipeline: ./debug-domain.ail.yaml
      prompt: "Investigate: {{ step.classify.response | extract(1) }}"
    - id: domain_2
      pipeline: ./debug-domain.ail.yaml
      prompt: "Investigate: {{ step.classify.response | extract(2) }}"
  on_error: continue     # or abort_all
  synthesize:
    prompt: |
      Combine results from parallel investigations:
      Domain 1: {{ step.domain_1.response }}
      Domain 2: {{ step.domain_2.response }}
      Check for conflicts between fixes.
```

### Design Questions to Resolve

1. **Isolation model** — Do parallel branches run in separate sessions? (Probably yes, like sub-pipelines)
2. **Failure semantics** — `abort_all` (fail-fast) vs `continue` (collect all results, report failures in synthesis)
3. **Resource contention** — Parallel file edits could conflict. Should branches get isolated worktrees?
4. **Max parallelism** — Hard limit on concurrent branches (like `MAX_SUB_PIPELINE_DEPTH`)
5. **Template access** — `{{ step.<parallel_id>.response }}` for each branch

### Context

This gap was discovered while reproducing [obra/superpowers](https://github.com/obra/superpowers) as AIL pipeline workflows. See `demo/superpowers/parallel-debug.ail.yaml` for the sequential workaround and PROPOSED comments showing intended syntax.
