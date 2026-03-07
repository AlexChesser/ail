## 6. Skills

A skill is a directory containing a `SKILL.md` file — natural language instructions that tell the model how to perform a specialised task. Skills follow the [Agent Skills open standard](https://agentskills.io), making any skill authored for Claude, Gemini CLI, GitHub Copilot, Cursor, or other compatible tools directly usable in `ail` without modification.

### 6.1 The Skill/Pipeline Distinction

| | Skill | Pipeline |
|---|---|---|
| **Format** | Markdown | YAML |
| **Read by** | The LLM | The `ail` runtime |
| **Contains** | Instructions, examples, guidelines | Control flow, sequencing, branching |
| **Scope** | How to think about a task | When to run it and what to do with the result |

### 6.2 Using a Skill in a Step

```yaml
# Local skill directory
- id: security_review
  skill: ./skills/security-reviewer/

# Parent directory skill
- id: org_review
  skill: ../org-skills/compliance-checker/

# Home directory skill
- id: personal_style
  skill: ~/skills/my-conventions/

# Built-in ail skill
- id: dry_check
  skill: ail/dry-refactor
```

### 6.3 Combining `skill:` and `prompt:`

A step may declare both. The skill provides standing instructions; the prompt provides the specific task for this invocation.

```yaml
- id: security_review
  skill: ./skills/security-reviewer/
  prompt: "{{ step.invocation.response }}"
  provider: frontier
  on_result:
    contains: "CLEAN"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Security findings require human review."
```

When both are present, skill content is provided as system/instruction context and the prompt is the user-level task.

### 6.4 Agent Skills Compatibility

`ail`'s built-in modules (§14) are implemented as Agent Skills-compliant `SKILL.md` packages — inspectable, forkable, and overridable. Any skill from the broader Agent Skills ecosystem is usable in `ail` by path reference.

---
