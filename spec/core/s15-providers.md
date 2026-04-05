## 15. Providers

> **Implementation status:** Partial. `defaults.model`, `defaults.provider.base_url`, and `defaults.provider.auth_token` are parsed and applied. Per-step `model:` and `resume:` overrides are implemented. Named provider aliases (`providers:` block) and provider string format (`vendor/model`) are not yet implemented.

### 15.1 Provider String Format

```yaml
provider: openai/gpt-4o
provider: anthropic/claude-opus-4-5
provider: groq/llama-3.1-70b-versatile
provider: cerebras/llama-3.3-70b
provider: fast       # named alias
provider: frontier   # named alias
```

### 15.2 Provider Aliases

Defined in `~/.config/ail/providers.yaml` or in a `providers` block in the pipeline file.

```yaml
providers:
  fast:     groq/llama-3.1-70b-versatile
  balanced: openai/gpt-4o-mini
  frontier: anthropic/claude-opus-4-5

defaults:
  provider: balanced
```

### 15.3 Credentials

Provider API keys are read from environment variables. `ail` never stores credentials. The expected environment variable names follow each provider's standard conventions (e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`). See your provider's documentation.

### 15.4 Session Resumption — `resume:`

By default, each pipeline step is a fresh, isolated invocation against its provider. Steps on different providers are always isolated from each other. Steps on the same provider are also isolated by default — no implicit session continuity is assumed.

When a step declares `resume: true`, `ail` uses the session ID from the most recent preceding step on the same provider within this pipeline run and passes it to the current step's invocation. This requests session continuity at the provider level — the LLM receives the conversation history from the prior step.

```yaml
- id: security_audit
  provider: anthropic/claude-opus-4-5
  resume: true
  prompt: "Now review the refactored code for security vulnerabilities."
```

**Scoping rule:** `resume: true` resumes the session from the **most recent preceding step on the same provider** in this pipeline run. If no preceding step on the same provider exists, the step behaves as a fresh session and a warning is emitted.

**Provider capability:** Session resumption depends on the provider supporting it. `resume: true` on a runner or provider that does not support session resumption raises a warning at parse time and falls back to an isolated invocation. Whether and how session resumption is implemented is defined in `RUNNER-SPEC.md`.

**Session ID capture:** `ail` captures the session ID returned by a provider invocation whenever the provider makes one available, and writes it to the pipeline run log.  This happens regardless of whether `resume: true` is declared — session IDs are always logged when present. `resume: true` consumes the most recently logged session ID for the same provider.

---
