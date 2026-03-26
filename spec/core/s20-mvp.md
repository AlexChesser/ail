## 20. MVP — v0.0.1 Scope

The goal of v0.0.1 is a working demo: one pipeline, one runner, one follow-up prompt, visibly running. Nothing more. This is the proof of concept that validates the core guarantee before any additional complexity is added.

**In scope for v0.0.1:**

| Feature | Notes |
|---|---|
| Single pipeline file (`.ail.yaml`) | No inheritance, no `FROM` |
| `pipeline:` array with ordered steps | Top-to-bottom execution only |
| `prompt:` field — inline string only | No file path resolution yet |
| `id:` field | Required for all steps |
| `provider:` field | At least one working provider |
| `on_result: contains` + `continue` / `pause_for_human` / `abort_pipeline` | Minimal branching. `pause_for_human` here is an **`on_result` action** (conditional HITL), not the standalone HITL gate step type. |
| `condition: always` and `condition: never` | Trivial conditions — proves the condition system exists |
| `{{ step.invocation.response }}` and `{{ last_response }}` | Core template variables |
| Passthrough mode when no `.ail.yaml` found | Safe default |
| `ail materialize` | Flattens a single-file pipeline — no inheritance to traverse yet, but establishes the command |
| Basic TUI — streaming stdout passthrough | Human can see the runner working |
| Pipeline run log — persisted to disk | Step responses durable before next step |
| Completion detection via process exit code 0 | For CLI runner steps |

**Explicitly out of scope for v0.0.1:**

- `FROM` inheritance and all hook operations
- `skill:` field
- `pipeline:` field (calling sub-pipelines)
- `action: pause_for_human` as a standalone step type (unconditional HITL gate — distinct from the `on_result` action of the same name)
- `context:` step type (shell execution)
- `on_result` multi-branch array syntax and `exit_code:` operator
- `condition:` expressions beyond `always` / `never`
- File path resolution for `prompt:`
- `defaults:` block
- `resume:` field
- Multiple named pipelines
- All built-in modules
- Everything in §22 Planned Extensions

**The v0.0.1 demo case:**

```yaml
version: "0.1"

pipeline:
  - id: dont_be_stupid
    prompt: "Review the above output. Fix anything obviously wrong or unnecessarily complex."
```

One file. One step. Always runs. Ships.

---

## v0.1 Scope — Benchmarking-Ready

The goal of v0.1 is the minimum set of primitives required to run the SWE-bench benchmarking experiment described in §21: a declared pipeline that runs a linter, a test suite, and a self-evaluation step, then reports a pass/fail result without human interaction.

**In scope for v0.1 (in addition to all v0.0.1 features):**

| Feature | Notes |
|---|---|
| `context: shell:` steps | Shell execution with stdout, stderr, exit code capture. See §5.5. |
| `on_result` multi-branch array syntax | First-match evaluation of multiple rules. See §5.4. |
| `on_result: exit_code:` operator | Integer and `any` (non-zero). Valid on `shell:` context steps only. |
| `on_result: always:` action | Unconditional branch. Canonical form — use as a key, not `match: always`. |
| `on_error` / `on_result` lifecycle | Defined: errors (runner crash, timeout) trigger `on_error`; runner returns (any exit) trigger `on_result`. See §16. |
| File path resolution for `prompt:` | `./`, `../`, `~/`, `/` prefixes. |
| All context step template variables | `{{ step.<id>.result }}`, `.stdout`, `.stderr`, `.exit_code`. |
| `--headless` flag | `pause_for_human` aborts (configurable: auto-approve). Exit code reflects pipeline success. Needed for automated/CI runs. |

**v0.1 target pipeline (benchmarking experiment):**

```yaml
version: "0.1"

pipeline:
  - id: lint
    context:
      shell: "cargo clippy -- -D warnings"
    on_result:
      - exit_code: 0
        action: continue
      - exit_code: any
        action: continue   # log failure; let fix_lint run

  - id: tests
    context:
      shell: "cargo test --quiet"
    on_result:
      - exit_code: 0
        action: break      # all good — exit cleanly
      - exit_code: any
        action: continue

  - id: fix_and_verify
    prompt: |
      Lint result (exit {{ step.lint.exit_code }}):
      {{ step.lint.result }}

      Test result (exit {{ step.tests.exit_code }}):
      {{ step.tests.result }}

      Fix all failures. Explain what you changed.
```

---
