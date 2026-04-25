## 3. File Format

### 3.0 CLI Invocation

The canonical way to run `ail` is a positional prompt argument:

```
ail "my prompt"
```

`--once <PROMPT>` is a long-form alias kept for backwards compatibility and for use in scripts where positional arguments may be ambiguous. Both forms are equivalent; they cannot be combined.

```
ail --once "my prompt"          # identical to the positional form
```

When a prompt is given and no subcommand is present, `ail` executes that prompt through the pipeline (or in passthrough mode if no pipeline is found). The output mode is selected by flags:

| Flag | Mode | Behaviour |
|---|---|---|
| _(none)_ | **lean** (default) | Prints the final response. When stdout is a TTY and the pipeline had at least one non-invocation step, appends `[ail: N steps in X.Xs]`. Omitted entirely for passthrough runs. |
| `--show-work` | **show-work** | After execution, prints a one-line summary per completed step, then the footer. Useful when you want to see what the pipeline did without the full verbose stream. |
| `--watch` | **watch** | Streams per-step progress to stderr as steps execute (step index, token counts). Use `--show-thinking` alongside `--watch` to include thinking blocks. |
| `--output-format json` | **json** | NDJSON event stream to stdout. Used by programmatic consumers (e.g. the VS Code extension). |

`--show-responses` is a hidden alias for `--watch` (kept for backwards compatibility).

If no prompt and no subcommand are given, `ail` prints a short usage hint and exits 0.

### 3.1 Discovery

`ail` looks for a pipeline definition file using the following resolution order. The first match wins.

1. Explicit path passed via `--pipeline <path>` CLI flag.
2. **Per-project last-used pointer** — `~/.ail/projects/<sha1_of_cwd>/last_pipeline`. Written by `config::load()` after every successful parse, so the next invocation in this project auto-resumes the user's choice. Stale pointers (target deleted) are silently ignored.
3. **Project default marker** — `<cwd>/.ail/default`, a single-line text file naming the default pipeline as a path relative to `.ail/` (e.g. `starter/default.yaml`). Hand-editable, commits cleanly, replaces a heavier YAML config for the one-line "which pipeline is the default" question.
4. **Subdir enumeration** — every `<cwd>/.ail/<sub>/*.{yaml,yml}` file (depth 2 only):
   - Exactly one candidate → resolve and use it.
   - Multiple candidates → present an interactive picker on a TTY; print the candidate list to stderr and exit 1 on a non-TTY.
   - Zero candidates → fall through.
   Files deeper than depth 2 (e.g. `.ail/<sub>/agents/foo.ail.yaml`) are sub-pipelines, not entry points; they remain reachable via `--pipeline` or by writing their relative path into the marker file.
5. `~/.config/ail/default.yaml` (user-level default).

If no file is found, `ail` runs in **passthrough mode**: the underlying agent behaves exactly as if `ail` were not present. This is the zero-configuration safe default.

When a user picks a pipeline interactively, the resulting load writes rule 2's last-used pointer, so subsequent invocations skip the picker and resume the same pipeline.

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

providers:                  # optional; named provider aliases (see §15) — not yet parsed
  fast:     groq/llama-3.1-70b-versatile
  frontier: anthropic/claude-opus-4-5

defaults:                   # optional; inherited by all steps
  model: gemma3:1b          # ✓ PARSED — model name passed as --model to the runner
                            #   may also be placed inside provider: (see below); provider wins if both set
  provider:                 # ✓ PARSED — provider connection details
    model: gemma3:1b        # ✓ PARSED — alternative location for model; takes precedence over defaults.model
    base_url: http://localhost:11434   # set as ANTHROPIC_BASE_URL in subprocess env
    auth_token: ollama                 # set as ANTHROPIC_AUTH_TOKEN in subprocess env
  timeout_seconds: 120      # ✓ PARSED — not yet enforced at runtime
  on_error: pause_for_human # not yet parsed
  tools:                    # ✓ PARSED — pipeline-wide tool policy fallback; per-step tools override entirely
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
