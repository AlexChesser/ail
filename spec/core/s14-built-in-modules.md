## 14. Built-in Modules

`ail`'s built-in modules are referenceable via `skill: ail/<name>`. Each is implemented as an Agent Skills-compliant `SKILL.md` package — inspectable, forkable, and overridable.

| Module | Description |
|---|---|
| `ail/janitor` | Context distillation. Compresses working context to reduce token usage. |
| `ail/dry-refactor` | Refactors code for DRY compliance. |
| `ail/security-audit` | Security-focused review. Pauses for human if findings exist. |
| `ail/test-writer` | Generates unit tests for new functions in the preceding response. |
| `ail/model-compare` | Runs the same prompt against two providers. Presents outputs side by side. |
| `ail/commit-checkpoint` | Prompts user to commit current changes before pipeline continues. |

```yaml
pipeline:
  - id: distill
    skill: ail/janitor

  - id: security
    skill: ail/security-audit
    on_result:
      contains: "VULNERABILITY"
      if_true:
        action: abort_pipeline
```

> **Note:** Skill parameterisation (`with:` or equivalent) is deferred. How a `SKILL.md` declares and receives parameters is an open question that will be resolved alongside structured output schema research. See §23.

---
