## 19. Runners & Adapters

> **Note:** This section describes the conceptual model for how `ail` connects to underlying CLI tools. The detailed contract for runner compliance is defined in a separate document — `RUNNER-SPEC.md` — which is currently a stub under active development. The interface described here should be considered directional, not final.

### What a Runner Is

A runner is the underlying CLI agent that `ail` wraps. It receives the human's prompt, produces a response, and signals completion. `ail` orchestrates everything that happens after that signal fires.

The runner is deliberately outside the pipeline language. `SPEC.md` defines what pipelines do. The runner is what the pipeline acts upon.

### Three Tiers of Runner Support

**Tier 1 — First-class runners**
Built-in adapters shipped with `ail` and maintained by the core team. Tested against every `ail` release. The runner's behaviour, output format, completion signal, and error codes are fully understood and handled.

Initial first-class runner: **Claude CLI** (`claude`). The v0.0.1 proof of concept targets Claude exclusively.

Roadmap for first-class support (not yet committed): Aider, OpenCode, Codex CLI, Gemini CLI, Qwen CLI, DeepSeek CLI, llama.cpp.

**Tier 2 — AIL-compliant runners**
Any CLI tool that implements the AIL Runner Contract defined in `RUNNER-SPEC.md`. A compliant runner works with `ail`'s built-in generic adapter without requiring a custom implementation. The tool author reads `RUNNER-SPEC.md` and ships their CLI accordingly. `ail` makes no guarantees about compliant runners beyond what the contract specifies.

**Tier 3 — Custom adapters**
Any CLI tool that does not implement the runner contract can be wrapped in a community-written or private adapter. Adapters implement the `Runner` trait defined in `ail`'s Rust core and are loaded at runtime as dynamic libraries. See `ARCHITECTURE.md` *(forthcoming)* for the trait interface and dynamic loading system.

### Runner Configuration

The active runner is declared in the pipeline file or in `~/.config/ail/config.yaml`:

```yaml
# In .ail.yaml
runner:
  id: claude
  command: claude
  args: ["--print"]         # invocation flags; runner-specific

# Or reference a custom adapter
runner:
  id: my-custom-runner
  adapter: ~/.ail/adapters/my-runner.so
```

If no runner is declared, `ail` defaults to the Claude CLI.

### The AIL Runner Contract (Summary)

The full contract is defined in `RUNNER-SPEC.md`. At a high level, a compliant runner must:

- Accept a prompt via a flag or stdin in non-interactive mode
- Write its response to stdout
- Exit with code `0` on success, non-zero on error
- Optionally declare supported capabilities (structured output, extended thinking, tool calls, session resumption) via a `--ail-capabilities` flag

Runners that implement the optional capability declarations unlock richer `ail` features — structured step output, thinking traces, tool call inspection, and `resume: true` support. Runners that implement only the minimum contract work with Tier 1 text-based pipeline features.

> **Note:** Session continuity behaviour — what "isolated" means per runner, and how session IDs are captured and passed for `resume: true` — is defined in `RUNNER-SPEC.md`, not here. The pipeline language declares intent; the runner contract defines mechanics.

### RunnerFactory and Per-Step Dispatch

`RunnerFactory` (`ail_core::runner::factory`) is the canonical way to obtain a runner by name at runtime. It is used by the executor for per-step runner dispatch.

**Runner selection hierarchy** (highest priority wins):

1. Per-step `runner:` field on the step in the pipeline YAML
2. `AIL_DEFAULT_RUNNER` environment variable
3. Hardcoded fallback: `"claude"` → `ClaudeCliRunner`

**Known runner names:**

| Name | Implementation | Notes |
|---|---|---|
| `claude` | `ClaudeCliRunner` | First-class; requires the `claude` binary |
| `stub` | `StubRunner` | Test/development only; returns a fixed response |

**Per-step runner: field**

Any `prompt:` step may declare a `runner:` field to override the default runner for that step only:

```yaml
pipeline:
  - id: review
    prompt: "Review the changes"
    # no runner: — uses the injected default (AIL_DEFAULT_RUNNER or claude)

  - id: audit
    prompt: "Security audit"
    runner: stub   # overrides for this step only
```

The override is resolved by `RunnerFactory::build(name, true)` — per-step runners are always headless (non-interactive subprocess invocations). An unrecognised runner name aborts the step with `RUNNER_NOT_FOUND` before the runner is called.

The default runner (no `runner:` field) is the runner injected into `execute()` — typically built by `RunnerFactory::build_default(headless)` in the binary entry point.

### Further Reading

- `RUNNER-SPEC.md` — The AIL Runner Contract. Read this if you are a CLI tool author who wants first-class `ail` compatibility.
- `ARCHITECTURE.md` *(forthcoming)* — The Rust trait interface and dynamic loading system. Read this if you are writing a custom runner adapter.

---
