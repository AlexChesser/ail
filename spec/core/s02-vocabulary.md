## 2. Concepts & Vocabulary

| Term | Definition |
|---|---|
| `pipeline` | A named, ordered sequence of steps defined in a `.ail.yaml` file. One pipeline is "active" per session. |
| `step` | A single unit of work within a pipeline. A step invokes a prompt, skill, sub-pipeline, or action, then optionally branches on the result. |
| `invocation` | The implicit first step of every pipeline. Represents the triggering event — a human prompt, an agent call, or a scheduler firing — and the runner's response to it. |
| `skill` | A directory containing a `SKILL.md` file — natural language instructions that tell the model how to perform a specialised task. Read by the LLM, not the runtime. |
| `trigger` | The event that causes the pipeline to begin executing. The default trigger is `invocation_prompt_complete`. |
| `session` | One running instance of an underlying agent (e.g. Aider, Claude Code) managed by `ail`. |
| `completion event` | The signal that the underlying runner has finished. For CLI tools, this is typically process exit with code 0. See §23 Open Questions. |
| `HITL gate` | A Human-in-the-Loop gate. The pipeline pauses and waits for explicit human input before continuing. |
| `pipeline run log` | The durable, persisted record of a pipeline execution. Written to disk before the next step runs. The authoritative source for template variable resolution. See §4.4. |
| `context` | The working memory passed between pipeline steps, accessed via the pipeline run log and template variables. |
| `provider` | The LLM backend a step routes its prompt to. May differ per step. |
| `condition` | A boolean expression evaluated before a step runs. If false, the step is skipped. |
| `on_result` | Declarative branching logic that fires after a step completes, based on the content of the response. |
| `FROM` | Keyword declaring that this pipeline inherits from another. Accepts a file path. Chainable. Must be acyclic. |
| `run_before` | Hook keyword. Inserts a step immediately before the named step ID in the inherited pipeline. |
| `run_after` | Hook keyword. Inserts a step immediately after the named step ID in the inherited pipeline. |
| `override` | Replaces a named step from an inherited pipeline entirely. |
| `disable` | Removes a named step from an inherited pipeline without replacing it. |
| `materialize-chain` | CLI command that traverses the full inheritance chain and writes the resolved pipeline to disk. |

---
