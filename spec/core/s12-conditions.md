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
| `matches` | Regular expression match. RHS is a `/PATTERN/FLAGS` literal (Rust `regex` crate syntax; unanchored). See below. |

#### Syntax Rules

- The **left-hand side** is a template expression (e.g. `{{ step.test.exit_code }}`) resolved at runtime.
- The **right-hand side** is a literal value. Surrounding single or double quotes are stripped: `'LGTM'` and `"LGTM"` both resolve to `LGTM`. The `matches` operator takes a `/PATTERN/FLAGS` regex literal instead — see below.
- Word-based operators (`contains`, `starts_with`, `ends_with`, `matches`) require whitespace boundaries — they are not confused with template variable names like `{{ step.contains_test.response }}`.
- Symbolic operators (`==`, `!=`) are matched outside `{{ }}` template blocks.

#### `matches` — Regular Expression Matching

The `matches` operator uses conventional regex-literal syntax: a leading `/`, the pattern, a closing `/`, and zero or more flag characters. No inline flag expressions, no clever escape rules — it reads the same way as regex literals in JavaScript, Perl, or Ruby.

```yaml
condition: '{{ step.test.response }} matches /^PASS\b/'
condition: '{{ step.build.stderr }} matches /error|warning/i'
condition: '{{ step.review.response }} matches /LGTM|SHIP IT/'
condition: '{{ step.lint.stdout }} matches /^E\d{4}/m'
```

**Syntax.** `/PATTERN/FLAGS`

| Flag | Meaning |
|---|---|
| `i` | Case-insensitive |
| `m` | Multiline — `^` and `$` match at line boundaries |
| `s` | Dotall — `.` matches newlines |

**Parsing rule.** The regex literal is delimited by the *first* `/` and the *last* `/` followed by zero or more flag characters (`[ims]*`) at end-of-string. Forward slashes inside the pattern do not need escaping as long as the literal isn't ambiguous: `/a/b/` matches the pattern `a/b` with no flags; `/a/b/i` matches `a/b` case-insensitively. When in doubt, escape as `\/`.

**Semantics.**

- **Engine:** Rust `regex` crate (linear-time; no backreferences or lookaround).
- **Anchoring:** unanchored by default. `/PASS/` matches `"tests PASSED"`. Use `^`/`$` for exact matching.
- **Case sensitivity:** case-sensitive unless the `i` flag is set.
- **Unsupported flags:** `g` (global) is rejected at parse time — matching in `on_result`/`condition:` is boolean, so a "global" flag has no meaning. `x` (verbose) and other Perl-style flags are not supported; use `(?x)` inline syntax if you need them.
- **Invalid regex:** compilation failure (including unsupported flags) is caught at parse time with `CONFIG_VALIDATION_FAILED`. A pipeline with a broken regex refuses to load.

**YAML quoting.** Prefer **single-quoted** YAML strings for conditions containing regex. Single quotes leave backslashes literal, so `\d`, `\b`, `\s` work as written:

```yaml
condition: '{{ step.x.response }} matches /\d{3}-\d{4}/'   # clean
condition: "{{ step.x.response }} matches /\\d{3}-\\d{4}/" # works, but doubled backslashes
```

#### Error Handling

- If the LHS template variable cannot be resolved (e.g. references a step that has not run), the pipeline aborts with a `CONDITION_INVALID` error (`ail:condition/invalid`).
- If the condition string is not a recognised named condition and does not contain a supported operator, validation fails with `CONFIG_VALIDATION_FAILED` at parse time.
- If a `matches` operator is present but its RHS fails to compile as a regex, validation fails with `CONFIG_VALIDATION_FAILED` at parse time.

#### Reuse in `on_result: expression:`

The `expression:` matcher on `on_result` (§5.4) reuses this grammar without modification. Any valid condition expression is a valid `on_result: expression:` expression and vice versa. When the grammar is extended (e.g. numeric operators for confidence-score gating — #130), the extension applies to both sites simultaneously.

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
