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
| `on_result: contains` + `continue` / `pause_for_human` / `abort_pipeline` | Minimal branching |
| `condition: always` and `condition: never` | Trivial conditions — proves the condition system exists |
| `{{ step.invocation.response }}` and `{{ last_response }}` | Core template variables |
| Passthrough mode when no `.ail.yaml` found | Safe default |
| `ail materialize-chain` | Flattens a single-file pipeline — no inheritance to traverse yet, but establishes the command |
| Basic TUI — streaming stdout passthrough | Human can see the runner working |
| Pipeline run log — persisted to disk | Step responses durable before next step |
| Completion detection via process exit code 0 | For CLI runner steps |

**Explicitly out of scope for v0.0.1:**

- `FROM` inheritance and all hook operations
- `skill:` field
- `pipeline:` field (calling sub-pipelines)
- `action: pause_for_human` (HITL gates)
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
