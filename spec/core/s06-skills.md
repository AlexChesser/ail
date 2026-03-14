## 6. Skills

`ail` implements the [Agent Skills open standard](https://agentskills.io/specification). Refer to that specification for `SKILL.md` format, frontmatter fields, directory structure, argument substitution, and path resolution. Any divergence from the standard is a bug; report it as an issue.

This section documents only what `ail` adds or changes.

### 6.1 The Skill/Pipeline Distinction

| | Skill | Pipeline |
|---|---|---|
| **Format** | Markdown | YAML |
| **Read by** | The LLM | The `ail` runtime |
| **Purpose** | How to think about a task | When to run it and what to do with the result |

### 6.2 Progressive Disclosure — ail's Deliberate Departure

The Agent Skills standard specifies that skill metadata (`name` + `description`) is always in context. `ail` deliberately departs from this — and the reason is the core of what `ail` is. Adding more instructions to a context saturation problem makes a larger context. A skill surfaced to the wrong step is subject to the same attention degradation as every other instruction competing for the middle of the window. `ail` operates at the layer that decides what goes into the context at all — selectively surfacing skill metadata only to the steps where it is relevant is that layer's job.

`ail` conforms to the Agent Skills **format** standard. Context injection timing is `ail`'s to control.

### 6.3 ail-Specific Behaviour

**`skill:` pipeline step.** The `SKILL.md` body is sent to the runner as the user-level prompt. See §5.3.

**REPL invocation.** `/skill-name [args]` in the `ail` REPL executes the skill and pauses for human review before returning control. Discovery order: project `.claude/skills/` → personal `~/.claude/skills/` → `ail/` built-ins.

**Loading skill instructions as system context.** To use a skill's content as passive context for a `prompt:` step rather than as a task, reference the file directly via `file:` in `append_system_prompt:` — `ail` does not support `skill:` entries in `append_system_prompt:`.

```yaml
- id: review
  append_system_prompt:
    - file: ./skills/security-reviewer/SKILL.md
  prompt: "{{ step.invocation.response }}"
```

---
