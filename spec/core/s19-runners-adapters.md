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

### Further Reading

- `RUNNER-SPEC.md` — The AIL Runner Contract. Read this if you are a CLI tool author who wants first-class `ail` compatibility.
- `ARCHITECTURE.md` *(forthcoming)* — The Rust trait interface and dynamic loading system. Read this if you are writing a custom runner adapter.

---
