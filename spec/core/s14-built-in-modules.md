## 14. Built-in Modules

> **Implementation status:** v0.3 — fully implemented. All four built-in modules (`ail/code_review`, `ail/test_writer`, `ail/security_audit`, `ail/janitor`) are registered in the skill registry at startup. Skill parameterisation (`with:`) is deferred — see §22.

`ail`'s built-in modules are referenceable via `skill: ail/<name>`. Each is implemented as a named prompt template registered in the skill registry at startup.

### 14.1 Available Modules

| Module | Description |
|---|---|
| `ail/code_review` | Reviews code for correctness, style, security, and performance. |
| `ail/test_writer` | Generates unit tests for functions in the preceding response. |
| `ail/security_audit` | Security-focused review. Includes "VULNERABILITY" keyword in findings for `on_result` branching. |
| `ail/janitor` | Context distillation. Compresses working context to reduce token usage. |

All built-in skills use `{{ last_response }}` to receive the output of the preceding step as input.

### 14.2 Usage Examples

```yaml
pipeline:
  - id: distill
    skill: ail/janitor

  - id: security
    skill: ail/security_audit
    on_result:
      - contains: "VULNERABILITY"
        action: abort_pipeline
```

### 14.3 Naming Convention

Built-in modules use underscores in their names (e.g. `ail/code_review`, not `ail/code-review`). This matches Rust identifier conventions and avoids ambiguity with path separators.

### 14.4 Future Work

> **Note:** Skill parameterisation (`with:` or equivalent) is deferred. How skills declare and receive parameters is an open question that will be resolved alongside structured output schema research. See §22. Additional built-in modules (e.g. `ail/dry_refactor`, `ail/model_compare`, `ail/commit_checkpoint`) will be added as the skill system matures.

---
