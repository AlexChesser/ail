# Context Transfer: oh-my-ail Classifier Fix & System Prompt Discovery

**Date:** 2026-04-07
**Purpose:** Carry forward findings from the oh-my-ail classifier debugging session.

## What was done

The oh-my-ail Sisyphus classifier was broken — it executed user prompts directly instead of classifying them. Three changes were made (all uncommitted):

### 1. `.ohmy.ail.yaml` — classification instructions moved to prompt field
The invocation step's `prompt:` now contains "Classify this request..." instructions followed by `{{ session.invocation_prompt }}`, rather than relying solely on `system_prompt: ./prompts/sisyphus.md`. Also added `tools: { disabled: true }` to prevent tool calls.

### 2. ail-core — `NoTools` policy variant added
- `ToolPermissionPolicy::NoTools` in `runner/mod.rs` → maps to `--tools ""` in `runner/claude.rs`
- `disabled: bool` field added to `ToolsDto` (dto.rs), `ToolPolicy` (domain.rs), wired through `validation.rs`
- `build_tool_policy` in `executor/helpers.rs` checks `disabled` first → returns `NoTools`
- All tests pass (263/263), clippy clean, fmt clean

### 3. Spec updated
- `spec/core/s05-step-specification.md` §5.8 — documents `tools: { disabled: true }` with rationale for small models
- `spec/runner/r02-claude-cli.md` — documents that `--system-prompt` appends (not replaces), documents `--tools ""` flag

## Critical discovery: Claude CLI system prompt behavior

**`--system-prompt` does NOT replace Claude CLI's base system prompt.** It appends. Even with `--bare` + `--tools ""`, Claude CLI injects date, environment info, and other context.

### Evidence

1. **Direct Ollama API test** (bypassing Claude CLI): `system_prompt: "Classify as TRIVIAL..."` + `think: false` → model correctly returns `TRIVIAL`
2. **Claude CLI with --bare + --tools "" + --system-prompt**: model ignores classification, tries to answer the user's question directly. Thinking block references "The current date is 2026-04-07" — content not in user's system prompt.

### Why this matters

ail's architecture promises deterministic per-step system prompts. The Claude CLI runner undermines this because:
- It always adds its own system context
- For small models (0.8B), the combined system prompt is too large to process
- Even for large models, the pipeline author doesn't have full control

### Paths forward

1. **Direct API runner** (recommended) — `OllamaRunner` or `HttpRunner` that calls provider APIs directly. Full control over system prompt, tools, model params (like qwen3.5's `think: false`). The `Runner` trait is already the seam. Trade-off: no built-in Claude CLI tools.

2. **Anthropic API runner** — Direct API calls to Claude without Claude CLI wrapper. Same full control benefit. Larger trade-off: must implement tool handling in ail-core.

3. ~~**Deeper `--bare` investigation**~~ — **RESOLVED.** Tested `--bare + --tools "" + --system-prompt "Classify as TRIVIAL..."` against qwen3.5:0.8b on 2026-04-08. Model ignored the classification instruction entirely and answered the question as a helpful assistant. It did NOT call tools (`--tools ""` works), but the system prompt injection from Claude CLI is sufficient to override a tiny model's instruction-following. Direct Ollama API with `think: false` and the same system prompt correctly returns `TRIVIAL`. **Conclusion: this is definitively a Claude CLI system prompt injection problem, not a model capability problem.** A direct API runner solves it completely.

### Second confound: qwen3.5 thinking mode

qwen3.5:0.8b defaults to "thinking mode" where all output goes to a `reasoning` field and `content` is empty. Claude CLI doesn't pass `think: false` to Ollama. With `think: false` via direct API, the model works correctly. A direct API runner would solve this too.

## Files changed (uncommitted)

```
demo/oh-my-ail/.ohmy.ail.yaml          — classifier prompt + tools: disabled
ail-core/src/runner/mod.rs              — NoTools variant
ail-core/src/runner/claude.rs           — --tools "" mapping
ail-core/src/config/dto.rs              — disabled field
ail-core/src/config/domain.rs           — disabled field
ail-core/src/config/validation.rs       — disabled wiring
ail-core/src/executor/helpers.rs        — NoTools in build_tool_policy
ail-core/tests/spec/s09_tool_permissions.rs — disabled: false in test constructions
spec/core/s05-step-specification.md     — §5.8 tools: disabled documentation
spec/runner/r02-claude-cli.md           — system prompt behavior + --tools "" docs
```

## ail-core CLAUDE.md update needed

Add to Key Types in `ail-core/CLAUDE.md`:
```
pub enum ToolPermissionPolicy { RunnerDefault, NoTools, Allowlist(...), Denylist(...), Mixed { ... } }
```
And note that `ToolPolicy.disabled` maps to `NoTools` → `--tools ""`.
