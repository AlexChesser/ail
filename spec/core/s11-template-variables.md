## 11. Template Variables

Prompt strings and file-based prompts may reference runtime context using `{{ variable }}` syntax. Variables resolve at step execution time from the persisted pipeline run log, not from in-memory state.

| Variable | Value |
|---|---|
| `{{ step.invocation.prompt }}` | The input that triggered this pipeline run. |
| `{{ step.invocation.response }}` | The runner's response before any pipeline steps ran. |
| `{{ last_response }}` | The full response from the immediately preceding step. |
| `{{ step.<id>.response }}` | The response from a specific named step in this pipeline run. |
| `{{ step.<id>.tool_calls }}` | The tool calls made by a specific named step (array). |
| `{{ session.tool }}` | The underlying runner name (e.g. `aider`, `claude-code`). |
| `{{ session.cwd }}` | The current working directory of the session. |
| `{{ pipeline.run_id }}` | Unique ID for this pipeline execution. |
| `{{ env.VAR_NAME }}` | An environment variable. Fails loudly if not set — use this form for required variables. Use `{{ env.VAR_NAME \| default("value") }}` to fall back to a string literal. v0.1 supports string literals only. File and inline fallbacks for complex JSON or prompt content are a planned extension — for now, declare required variables explicitly and handle optional configuration via separate steps. |

> **Note:** There are no convenience aliases. All variable references use the dot-path structure above. This keeps the mental model consistent — every step, including `invocation`, is accessed the same way.

**Skipped step variables:** If a template variable references a step that was skipped by its `condition`, `ail` raises a **parse-time error** if the reference is unconditional, or returns an empty string if the referencing step itself has a matching condition guard. Silently empty references are never permitted.

**Future step variables:** Template variables may only reference steps that have already run. A reference to a step that has not yet executed at the point of resolution raises a fatal parse error.

---
