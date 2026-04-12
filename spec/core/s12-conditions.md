## 12. Conditions

> **Implementation status:** Full. `never`, `always`, and expression conditions are implemented.

The `condition` field allows declarative skip logic. If false, the step is skipped and the pipeline continues.

### 12.1 Named Conditions

| Expression | Meaning |
|---|---|
| `always` | Always true. Equivalent to omitting `condition`. |
| `never` | Always false. Identical to `disabled: true`. |

### 12.2 Expression Syntax

Expression conditions evaluate a comparison between a left-hand side (typically a template variable) and a right-hand side (a literal value). The LHS is template-resolved at runtime before comparison.

```yaml
condition: "{{ step.test.exit_code }} == 0"
condition: "{{ step.review.response }} contains 'LGTM'"
condition: "{{ step.build.exit_code }} != 0"
condition: "{{ step.check.response }} starts_with 'PASS'"
condition: "{{ step.check.response }} ends_with 'done'"
```

#### Supported Operators

| Operator | Meaning |
|---|---|
| `==` | String equality (after trimming whitespace) |
| `!=` | String inequality (after trimming whitespace) |
| `contains` | Case-insensitive substring check |
| `starts_with` | Case-insensitive prefix check |
| `ends_with` | Case-insensitive suffix check |

#### Syntax Rules

- The **left-hand side** is a template expression (e.g. `{{ step.test.exit_code }}`) resolved at runtime.
- The **right-hand side** is a literal value. Surrounding single or double quotes are stripped: `'LGTM'` and `"LGTM"` both resolve to `LGTM`.
- Word-based operators (`contains`, `starts_with`, `ends_with`) require whitespace boundaries — they are not confused with template variable names like `{{ step.contains_test.response }}`.
- Symbolic operators (`==`, `!=`) are matched outside `{{ }}` template blocks.

#### Error Handling

- If the LHS template variable cannot be resolved (e.g. references a step that has not run), the pipeline aborts with a `CONDITION_INVALID` error (`ail:condition/invalid`).
- If the condition string is not a recognised named condition and does not contain a supported operator, validation fails with `CONFIG_VALIDATION_FAILED` at parse time.

#### Template Variables in Conditions

All template variables from §11 are available:

```yaml
# Check exit code of a shell step
condition: "{{ step.build.exit_code }} == 0"

# Check if a prompt step response contains a keyword
condition: "{{ step.review.response }} contains 'LGTM'"

# Check stdout of a context step
condition: "{{ step.lint.stdout }} contains 'no warnings'"

# Compare against environment variable
condition: "{{ env.DEPLOY_TARGET }} == production"
```

---
