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

> **Note:** All variable references use the dot-path structure above. `{{ session.invocation_prompt }}` is a supported but **deprecated** alias for `{{ step.invocation.prompt }}`; prefer the canonical form. No other aliases exist.

**Skipped step variables:** If a template variable references a step that was skipped by its `condition`, `ail` raises a **parse-time error** if the reference is unconditional, or returns an empty string if the referencing step itself has a matching condition guard. Silently empty references are never permitted.

**Future step variables:** Template variables may only reference steps that have already run. A reference to a step that has not yet executed at the point of resolution raises a fatal parse error.

---
