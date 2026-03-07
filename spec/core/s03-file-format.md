## 3. File Format

### 3.1 Discovery

`ail` looks for a pipeline definition file using the following resolution order. The first match wins.

1. Explicit path passed via `--pipeline <path>` CLI flag.
2. `.ail.yaml` in the current working directory.
3. `.ail/default.yaml` in the current working directory.
4. `~/.config/ail/default.yaml` (user-level default).

If no file is found, `ail` runs in **passthrough mode**: the underlying agent behaves exactly as if `ail` were not present. This is the zero-configuration safe default.

The discovery order is significant beyond file resolution — it is the **authority order** that governs hook precedence in inherited pipelines. See §8.

### 3.2 Top-Level Structure

```yaml
# .ail.yaml
version: "0.1"              # required; must match supported spec version

FROM: ./base.yaml           # optional; inherit from another pipeline (see §7)
                            # accepts file paths only — see §22 for future URI support

meta:                       # optional block
  name: "My Quality Gates"
  description: "DRY refactor + security audit on every output"
  author: "alex@example.com"

providers:                  # optional; named provider aliases (see §15)
  fast:     groq/llama-3.1-70b-versatile
  frontier: anthropic/claude-opus-4-5

defaults:                   # optional; inherited by all steps
  provider: openai/gpt-4o
  timeout_seconds: 120
  on_error: pause_for_human
  tools:                    # pipeline-wide tool policy; overridable per step
    allow: [Read, Glob, LS]
    deny: [WebFetch]

pipeline:                   # required; ordered list of steps
  - id: dry_refactor
    prompt: "Refactor the code above to eliminate unnecessary repetition."

  - id: security_audit
    prompt: "Review the changes for common security vulnerabilities."
```

**Version field:** The `version` field declares the minimum `ail` runtime version required to execute this pipeline. Each file in a `FROM` chain makes its own independent version declaration. The active `ail` runtime must support all versions declared anywhere in the resolved chain — if any file declares a version higher than the runtime supports, `ail` raises a fatal parse error identifying the conflicting file and recommending a runtime upgrade. There is no constraint on relative versions between files in the chain.

---
