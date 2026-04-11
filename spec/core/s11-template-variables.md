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
| `{{ step.<id>.tool_calls }}` | The tool calls made by a specific named step (array). |
| `{{ session.tool }}` | The underlying runner name (e.g. `aider`, `claude-code`). |
| `{{ session.cwd }}` | The current working directory of the session. |
| `{{ pipeline.run_id }}` | Unique ID for this pipeline execution. |
| `{{ env.VAR_NAME }}` | An environment variable. Fails loudly if not set — use this form for required variables. Use `{{ env.VAR_NAME \| default("value") }}` to fall back to a string literal. v0.1 supports string literals only. File and inline fallbacks for complex JSON or prompt content are a planned extension — for now, declare required variables explicitly and handle optional configuration via separate steps. |

> **Note:** There are no convenience aliases. All variable references use the dot-path structure above. This keeps the mental model consistent — every step, including `invocation`, is accessed the same way.

**Skipped step variables:** If a template variable references a step that was skipped by its `condition`, `ail` raises a **parse-time error** if the reference is unconditional, or returns an empty string if the referencing step itself has a matching condition guard. Silently empty references are never permitted.

**Future step variables:** Template variables may only reference steps that have already run. A reference to a step that has not yet executed at the point of resolution raises a fatal parse error.

---
