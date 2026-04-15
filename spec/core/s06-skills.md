## 6. Skills

> **Implementation status:** v0.3 — fully implemented. The built-in skill registry is populated at startup. Project-defined skills (SKILL.md files) are resolved from the project directory. See §14 for the catalogue of built-in modules.

Skills are reusable, named prompt templates invoked via the `skill:` step body. Built-in skills live under the `ail/` namespace and are registered in the skill registry at startup. See §14 for the catalogue of built-in modules.

### 6.1 The Skill/Pipeline Distinction

| | Skill | Pipeline |
|---|---|---|
| **Format** | Named prompt template (built-in or project-defined) | YAML |
| **Read by** | The `ail` runtime resolves name → prompt → runner | The `ail` runtime |
| **Purpose** | Reusable task template | Step sequencing and control flow |

### 6.2 `skill:` Step Body

A `skill:` step declares the skill name as its primary field value:

```yaml
pipeline:
  - id: review
    skill: ail/code_review
```

At execution time, the runtime:

1. Looks up the skill name in the skill registry.
2. Retrieves the skill's prompt template.
3. Resolves template variables (e.g. `{{ last_response }}`) in the prompt template.
4. Sends the resolved prompt to the runner.
5. Records a `TurnEntry` with the resolved prompt and runner response.

**Unknown skill names** produce a `SKILL_UNKNOWN` (`ail:skill/unknown`) error at execution time — not at parse time. This allows pipeline files to reference skills that may be registered at runtime.

**Empty skill names** are rejected at validation time with `CONFIG_VALIDATION_FAILED`.

### 6.3 Skill Template Variables

Skill prompt templates support the same template variables as `prompt:` steps (see §11). The most common pattern is `{{ last_response }}`, which injects the output of the preceding step as input to the skill.

### 6.4 Tool and Model Overrides

Skill steps support all the same per-step overrides as prompt steps:

- `tools:` — tool allow/deny lists (§5.8)
- `model:` — per-step model override (§15)
- `runner:` — per-step runner override (§19)
- `on_result:` — declarative branching after completion (§5.4)
- `append_system_prompt:` — system prompt extensions (§5.9)
- `system_prompt:` — full system prompt override (§5.9)
- `resume: true` — resume previous runner session (§15.4)
- `condition:` — conditional execution (§12)

### 6.5 Loading Skill Instructions as System Context

To use a skill's content as passive context for a `prompt:` step rather than as a task, reference the file directly via `file:` in `append_system_prompt:` — `ail` does not support `skill:` entries in `append_system_prompt:`.

```yaml
- id: review
  append_system_prompt:
    - file: ./skills/security-reviewer/SKILL.md
  prompt: "{{ step.invocation.response }}"
```

---
