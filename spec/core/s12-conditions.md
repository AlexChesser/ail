## 12. Conditions

The `condition` field allows declarative skip logic. If false, the step is skipped and the pipeline continues.

### 12.1 Built-in Conditions

| Expression | Meaning |
|---|---|
| `if_code_changed` | True if the runner's response contains a code block. |
| `if_files_modified` | True if the runner modified files on disk. |
| `if_last_response_empty` | True if the previous step's response was blank. |
| `if_first_run` | True if this is the first pipeline run in this session. |
| `always` | Always true. Equivalent to omitting `condition`. |
| `never` | Always false. Identical to `disabled: true`. |

### 12.2 Expression Syntax

> **Status: Deferred — not in v0.1 scope.**

A general condition expression language — supporting dot-path comparisons, step response checks, and logical operators — is planned for a future version. The named conditions in §12.1 cover the common cases and are the only supported form in the current implementation.

```yaml
# DEFERRED — not yet implemented
condition: "context.file_count > 0"
condition: "step.security_audit.response contains 'VULNERABILITY'"
condition: "if_code_changed and not if_first_run"
```

The named conditions (`if_code_changed`, `if_files_modified`, etc.) are fully supported and are the recommended approach.

---
