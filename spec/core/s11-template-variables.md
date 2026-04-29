## 11. Template Variables

> **Implementation status:** Implemented. All documented template variables resolve correctly. `step.<id>.result`, `step.<id>.stdout`, `step.<id>.stderr`, and `step.<id>.exit_code` are fully implemented for context steps. Unresolved variables abort with a typed error — never silently empty.

Prompt strings, file-based prompts, and `pipeline:` paths may reference runtime context using `{{ variable }}` syntax. Variables resolve at step execution time from the persisted pipeline run log, not from in-memory state.

| Variable | Value |
|---|---|
| `{{ step.invocation.prompt }}` | The input that triggered this pipeline run. |
| `{{ step.invocation.response }}` | The runner's response before any pipeline steps ran. |
| `{{ last_response }}` | The full response from the immediately preceding step. |
| `{{ step.<id>.response }}` | The response from a specific named `prompt:` step in this pipeline run. |
| `{{ step.<id>__on_result.response }}` | The response from a sub-pipeline triggered by an `on_result: pipeline:` branch on step `<id>`. The derived ID `<id>__on_result` is used to avoid shadowing the parent step's own response in the turn log. |
| `{{ step.<id>.result }}` | Output of a `context:` step. For `shell:`: stdout+stderr concatenated. For `mcp:`: tool output. |
| `{{ step.<id>.stdout }}` | Standard output of a `shell:` context step. |
| `{{ step.<id>.stderr }}` | Standard error of a `shell:` context step. |
| `{{ step.<id>.exit_code }}` | Exit code of a `shell:` context step, as a string. |
| `{{ step.<id>.tool_calls }}` | Tool call and result events from a specific named `prompt:` step, serialised as a JSON array. Empty array for context/action/sub-pipeline steps. |
| `{{ step.<id>.modified }}` | Human-modified output from a `modify_output` HITL gate step (§13.2). Only available for steps with `action: modify_output` that produced a turn entry (i.e., the gate was not skipped). |
| `{{ session.tool }}` | The runner name resolved for the currently executing step (e.g. `claude`, `ollama`). Reflects per-step `runner:` overrides — updated at the start of each Prompt step. |
| `{{ session.cwd }}` | The current working directory of the session. |
| `{{ pipeline.run_id }}` | Unique ID for this pipeline execution. |
| `{{ env.VAR_NAME }}` | An environment variable. Fails loudly if not set — use this form for required variables. Use `{{ env.VAR_NAME \| default("value") }}` to fall back to a string literal. v0.1 supports string literals only. File and inline fallbacks for complex JSON or prompt content are a planned extension — for now, declare required variables explicitly and handle optional configuration via separate steps. |
| `{{ step.<id>.items }}` | Validated array from a step that declared `output_schema: type: array` (§26). Only available for steps with an array output schema. Referencing `.items` on a step without an array schema is a `TEMPLATE_UNRESOLVED` error. |
| `{{ do_while.iteration }}` | Inside a `do_while:` step body (§27): the current 0-based iteration index. Not available outside a `do_while:` body. |
| `{{ do_while.max_iterations }}` | Inside a `do_while:` step body (§27): the declared `max_iterations` value. Not available outside a `do_while:` body. |
| `{{ step.<id>.index }}` | Number of iterations completed by a `do_while:` loop step (§27). Only available on do_while steps after the loop completes. |
| `{{ step.<loop_id>::do_while[N].<step_id>.<field> }}` | Indexed iteration access (§27.4): reference a specific iteration's inner step result by 0-based index. Not yet implemented — produces `TEMPLATE_UNRESOLVED` until support is added. |
| `{{ for_each.item }}` | Inside a `for_each:` step body (§28): the current item value. If `as:` is set, also available as `{{ for_each.<as_name> }}` (e.g. `{{ for_each.task }}` when `as: task`). Not available outside a `for_each:` body. |
| `{{ for_each.index }}` | Inside a `for_each:` step body (§28): the current 0-based item index. |
| `{{ for_each.total }}` | Inside a `for_each:` step body (§28): the total number of items in the collection. |

> **Note:** All variable references use the dot-path structure above. `{{ session.invocation_prompt }}` and `{{ session.invocation.prompt }}` are supported but **deprecated** aliases for `{{ step.invocation.prompt }}`; prefer the canonical form. No other aliases exist.

**Skipped step variables:** If a template variable references a step that was skipped by its `condition`, `ail` raises a **parse-time error** if the reference is unconditional, or returns an empty string if the referencing step itself has a matching condition guard. Silently empty references are never permitted.

**Future step variables:** Template variables may only reference steps that have already run. A reference to a step that has not yet executed at the point of resolution raises a fatal parse error.

### 11.1 Examples

Reference the original prompt and prior step responses:

```yaml
pipeline:
  - id: plan
    prompt: "Outline an implementation plan for: {{ step.invocation.prompt }}"
  - id: critique
    prompt: |
      Review this plan and list risks:

      {{ step.plan.response }}
```

Inject the result of a deterministic context step:

```yaml
pipeline:
  - id: gather_diff
    context:
      shell: "git diff --staged"
  - id: review
    prompt: |
      Review the following staged diff:

      {{ step.gather_diff.stdout }}
```

Branch on a context step's exit code via a condition:

```yaml
pipeline:
  - id: tests
    context:
      shell: "cargo nextest run"
  - id: triage
    condition: "{{ step.tests.exit_code }} != '0'"
    prompt: "Tests failed. Triage:\n\n{{ step.tests.stderr }}"
```

Read an environment variable with a literal default:

```yaml
pipeline:
  - id: greet
    prompt: 'Greet the user named "{{ env.USER_NAME | default("friend") }}".'
```

Use loop variables inside a `for_each:` body:

```yaml
pipeline:
  - id: implement_each
    for_each:
      over: "{{ step.plan.items }}"
      as: task
      steps:
        - id: implement
          prompt: "Step {{ for_each.index }}/{{ for_each.total }}: {{ for_each.task }}"
```

A complete, runnable pipeline tying the patterns together — pinned by
the CI check (the leading `# spec:validate` comment opts it in), so
a regression here fails the build:

```yaml
# spec:validate
version: "0.1"

pipeline:
  - id: gather_diff
    context:
      shell: "git diff --staged"

  - id: review
    prompt: |
      Review the staged diff and list any concerns:

      {{ step.gather_diff.stdout }}

  - id: triage
    condition: "{{ step.gather_diff.exit_code }} != '0'"
    prompt: "git diff exited non-zero — investigate:\n\n{{ step.gather_diff.stderr }}"
```

---
