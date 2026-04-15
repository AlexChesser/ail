# Design: Sampling Parameter Control (#120)

## Context

This document captures the design decisions and implementation plan for
per-step LLM sampling parameter control. The spec draft lives at
`spec/core/s30-sampling-parameters.md`.

## Design Decisions

### D1: Which parameters to expose

**Decision**: Six first-class fields: `temperature`, `top_p`, `top_k`,
`max_tokens`, `stop_sequences`, `thinking`.

**Rationale**: These cover the universal set (temperature/top_p/max_tokens),
the important-but-not-universal set (top_k, stop_sequences), and the
increasingly critical reasoning toggle (thinking). No `extra: HashMap`
escape hatch in v1 — it adds complexity without clear demand. Can be added
later as a backwards-compatible extension.

**Alternative rejected**: Flat fields on Step (e.g., `step.temperature`).
Pollutes the step namespace. A `sampling:` block groups related concerns.

### D2: YAML syntax — reusable `sampling:` block at three scopes

**Decision**: `sampling:` is a single reusable block type that attaches at
three scopes:

1. `defaults.sampling:` — pipeline-wide baseline
2. `defaults.provider.sampling:` (now) / `providers.<name>.sampling:` (§15) — attached to a provider profile
3. per-step `sampling:` — override for a single step

Merge: `defaults.sampling.merge(provider.sampling).merge(step.sampling)` —
field-level, right-wins.

```yaml
defaults:
  provider:
    model: anthropic/claude-sonnet-4-5
    sampling: { temperature: 0.3 }    # attached to this provider
  sampling:
    max_tokens: 4096                  # pipeline-wide, orthogonal to provider

pipeline:
  - id: creative
    prompt: "..."
    sampling: { temperature: 0.9 }    # per-step override
```

**Rationale**: Pairing sampling with a provider profile means "define the
behavior once, reference by name." When §15 named providers land, authors
can declare `creative` and `precise` profiles and swap behavior with a
single field change (`provider: creative` → `provider: precise`), bringing
matched temperature, top_p, and thinking settings along. For now, even a
single `defaults.provider.sampling:` block colocates sampling intent with
the model it was tuned for. The pipeline-level `defaults.sampling:`
remains for cross-provider baselines (e.g., "always cap max_tokens").

### D3: Runner pass-through — first-class on InvokeOptions

**Decision**: Add `sampling: Option<SamplingConfig>` directly to `InvokeOptions`,
NOT inside the extensions box.

**Rationale**: Sampling parameters are universal — every runner understands
temperature and max_tokens in concept. They aren't runner-specific like
`base_url` or `auth_token`. Making them first-class on `InvokeOptions`
means runners don't need to downcast. The extensions box remains for
truly runner-specific data.

### D4: Keep sampling separate from ProviderConfig

**Decision**: New `SamplingConfig` struct, parallel to but separate from
`ProviderConfig`.

**Rationale**: `ProviderConfig` answers "where and how to connect" (model,
base_url, auth_token, timeouts). `SamplingConfig` answers "how to generate"
(temperature, top_p, max_tokens). They're conceptually distinct. Merging
them would make `ProviderConfig` a grab-bag.

Both structs follow the same `merge()` pattern: right-hand field wins,
absent fields fall through.

### D5: No CLI override for sampling

**Decision**: No `--temperature` flag on `ail` CLI.

**Rationale**: Sampling is a pipeline design concern. The pipeline author
chose temperature=0.3 for a reason. Runtime overrides via CLI flags would
undermine that intent. `--model` is the appropriate runtime knob — it changes
the model, which has its own default sampling behavior.

If demand emerges, `--sampling-override temperature=0.9` could be added later
as an explicit "I know what I'm doing" escape hatch.

### D6: Best-of-N is composition, not primitive

**Decision**: No built-in best-of-N. Compose with `for_each:` + selection step.

**Rationale**: Best-of-N is a pattern, not a primitive. It composes from
`for_each:` (generate N), `output_schema:` (structured candidates), and a
final selection step with `temperature: 0.0`. Adding a built-in would
duplicate what the loop system already does.

### D7: `thinking` is a decimal fraction in [0.0, 1.0]

**Decision**: `sampling.thinking: f64` in the range [0.0, 1.0]. `0.0` = off,
`1.0` = max. YAML booleans are accepted as aliases: `true` → `1.0`,
`false` → `0.0`. Each runner quantizes to its own supported granularity.

**Runner quantization**:
- **ClaudeCliRunner**: quartiles → `--effort low|medium|high|max`; `0.0` omits the flag entirely.
- **HttpRunner / Ollama**: `>= 0.5` → `"think": true`; `< 0.5` → `"think": false`.
- **Future Anthropic API with `budget_tokens`**: `budget_tokens = thinking * PROVIDER_MAX_BUDGET`, no information loss.
- **Future ail-native runner**: passes the float through as-is.

**Rationale**: Runners today have wildly different thinking granularity —
boolean (Ollama), 4-level enum (Claude CLI), continuous budget
(Anthropic API direction). A float expresses author intent once and
every runner quantizes on its own terms. Upgrades in runner granularity
(e.g., Ollama adding intensity levels) don't require any pipeline change.
The author wrote `thinking: 0.7` once and it still means the same thing.

This supersedes the current `HttpRunnerConfig.think` as the per-step
control. The config-level `think` remains as a runner-wide default.

### D8: Unsupported parameters warn, never error

**Decision**: Runners emit `tracing::warn!` for unsupported sampling params.
Never `AilError`.

**Rationale**: The acceptance criteria explicitly require this. It enables
pipeline portability — a pipeline authored for the HTTP runner (which supports
all params) can run on ClaudeCliRunner (which supports none today) with
warnings instead of failures.

## Implementation Plan

### Phase 1: Data model (DTO + Domain + Validation)

**Files to change**:

1. **`ail-core/src/config/dto.rs`**
   - Add `SamplingDto` struct with all six fields
   - Add custom deserializer `deserialize_thinking` that accepts f64 OR bool
     (bool `true`→1.0, `false`→0.0); wire via `#[serde(deserialize_with = ...)]`
   - Add `sampling: Option<SamplingDto>` to `StepDto`
   - Add `sampling: Option<SamplingDto>` to `DefaultsDto` (pipeline-scope)
   - Add `sampling: Option<SamplingDto>` to `ProviderDto` (provider-scope)

2. **`ail-core/src/config/domain.rs`**
   - Add `SamplingConfig` struct (Debug, Clone, Default) with all six fields,
     `thinking: Option<f64>`
   - Add `SamplingConfig::merge(self, other) -> SamplingConfig`
   - Add `SamplingConfig::is_empty()` helper
   - Add `sampling: Option<SamplingConfig>` to `ProviderConfig` (provider-scope)
   - Extend `ProviderConfig::merge()` to field-merge the inner sampling
   - Add `sampling_defaults: Option<SamplingConfig>` to `Pipeline` (pipeline-scope)
   - Add `sampling: Option<SamplingConfig>` to `Step` (step-scope)

3. **`ail-core/src/config/validation/mod.rs`** (+ new `sampling.rs` submodule)
   - Add `validate_sampling(dto: Option<SamplingDto>) -> Result<Option<SamplingConfig>>`
   - Range checks: temperature [0.0, 2.0], top_p [0.0, 1.0], top_k ≥ 1,
     max_tokens ≥ 1, thinking [0.0, 1.0]
   - Warn if `sampling:` appears on a context/action step
   - Wire into `validate_steps()` step construction
   - Wire into pipeline-level validation for `defaults.sampling` (pipeline-scope)
   - Wire into provider validation for `defaults.provider.sampling` (provider-scope)

### Phase 2: Executor plumbing

**Files to change**:

4. **`ail-core/src/runner/mod.rs`**
   - Add `sampling: Option<SamplingConfig>` to `InvokeOptions`
   - Update `InvokeOptions::default()` (already None)

5. **`ail-core/src/executor/helpers/runner_resolution.rs`**
   - Add `resolve_step_sampling(session: &Session, step: &Step) -> Option<SamplingConfig>`
   - Merge chain (low → high precedence):
     1. `session.pipeline.sampling_defaults` (pipeline-scope)
     2. `session.pipeline.defaults.sampling` (provider-scope, current single provider)
     3. `step.sampling` (step-scope)
   - Returns `None` if merged config is empty (nothing was set anywhere)
   - When §15 named providers land, step 2 resolves against the step's
     chosen provider profile instead of the single `defaults.provider`.

6. **`ail-core/src/executor/dispatch/prompt.rs`**
   - Call `resolve_step_sampling()` and set `options.sampling`

7. **`ail-core/src/executor/dispatch/skill.rs`**
   - Same as prompt.rs — skill steps also invoke the runner

### Phase 3: Runner implementations

**Files to change**:

8. **`ail-core/src/runner/claude/mod.rs`**
   - In `build_subprocess_spec()`:
     - Quantize `thinking: f64` to `--effort`:
       - `0.0` → omit flag (CLI default applies)
       - `(0.0, 0.25]` → `--effort low`
       - `(0.25, 0.50]` → `--effort medium`
       - `(0.50, 0.75]` → `--effort high`
       - `(0.75, 1.0]` → `--effort max`
     - Warn for each of `temperature`, `top_p`, `top_k`, `max_tokens`,
       `stop_sequences` when set — "not supported by claude CLI; ignoring"
   - The warning messages should be rate-limited or one-shot per field per
     run to avoid log spam (stretch goal — start with straight warnings).

9. **`ail-core/src/runner/http.rs`**
   - Add sampling fields to `ChatRequest`: `temperature`, `top_p`, `top_k`,
     `max_tokens`, `stop` (mapped from `stop_sequences`)
   - In `invoke()`: read from `options.sampling`, populate `ChatRequest` fields
   - Quantize `thinking` → `think` boolean: `>= 0.5` → `true`, `< 0.5` → `false`.
     Overrides `self.config.think` for that invocation.
   - Warn on `top_k` if the endpoint is not known to support it (optional)

10. **`ail-core/src/runner/plugin/protocol_runner.rs`**
    - Include `sampling` in the `invoke` JSON-RPC params

11. **`ail-core/src/runner/codex/mod.rs`**
    - Warn on sampling params (codex CLI doesn't support them)

12. **`ail-core/src/runner/dry_run.rs`**
    - No change needed (ignores all options)

### Phase 4: Session / Turn log

**Files to change**:

13. **`ail-core/src/session/state.rs`** (or `turn_log.rs`)
    - Add `sampling: Option<SamplingConfig>` to `TurnEntry`
    - Serialize only non-None fields in NDJSON output

### Phase 5: Materialize

**Files to change**:

14. **`ail-core/src/materialize.rs`**
    - Render `sampling:` block with `# origin` comments

### Phase 6: Tests

**Files to create/change**:

15. **`ail-core/tests/spec/s30_sampling.rs`**
    - Parse-time: valid sampling configs parse correctly
    - Parse-time: out-of-range values produce validation errors
    - Parse-time: sampling on context step produces warning
    - Merge: pipeline defaults + step override merges correctly
    - Executor: sampling flows through to InvokeOptions (use RecordingStubRunner)
    - Executor: absent sampling means None on InvokeOptions

16. **`ail-core/tests/fixtures/`**
    - Add `sampling_basic.yaml`, `sampling_override.yaml` fixtures

### Phase 7: Spec and docs

17. **`spec/core/s30-sampling-parameters.md`** — already drafted (this PR)
18. **`spec/README.md`** — add §30 row to the table
19. **`ail-core/CLAUDE.md`** — update Key Types for SamplingConfig, Step, Pipeline, InvokeOptions
20. **`CLAUDE.md`** — update template variable table if needed (no new vars expected)

## Struct Sketches

```rust
// ── DTO (dto.rs) ──────────────────────────────────────────────────────
// `thinking` is a float in [0.0, 1.0]. YAML booleans are accepted:
//   true  → 1.0
//   false → 0.0
// This is implemented via a custom Deserialize (e.g. a small `ThinkingDto`
// enum-deserialize that accepts f64 OR bool and normalizes to f64) OR a
// serde `#[serde(deserialize_with = "...")]` helper. Either way, the
// domain-level `SamplingConfig.thinking` is a plain `Option<f64>`.

#[derive(Debug, Default, Deserialize)]
pub struct SamplingDto {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u64>,
    pub max_tokens: Option<u64>,
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_thinking")]
    pub thinking: Option<f64>,  // accepts f64 | bool
}

// fn deserialize_thinking<'de, D>(d: D) -> Result<Option<f64>, D::Error>
// matches on visitor: f64 passes through; bool → 1.0 / 0.0.

// Attached to ProviderDto (provider-scope)
#[derive(Debug, Default, Deserialize)]
pub struct ProviderDto {
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub read_timeout_seconds: Option<u64>,
    pub max_history_messages: Option<usize>,
    pub sampling: Option<SamplingDto>,  // NEW: provider-attached sampling
}

// Attached to DefaultsDto (pipeline-scope) AND StepDto (step-scope)
// Both get: pub sampling: Option<SamplingDto>

// ── Domain (domain.rs) ────────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct SamplingConfig {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u64>,
    pub max_tokens: Option<u64>,
    pub stop_sequences: Option<Vec<String>>,
    pub thinking: Option<f64>,  // validated [0.0, 1.0] at parse time
}

impl SamplingConfig {
    /// Merge `other` on top of `self`. `other` fields take precedence.
    pub fn merge(self, other: SamplingConfig) -> SamplingConfig {
        SamplingConfig {
            temperature: other.temperature.or(self.temperature),
            top_p: other.top_p.or(self.top_p),
            top_k: other.top_k.or(self.top_k),
            max_tokens: other.max_tokens.or(self.max_tokens),
            stop_sequences: other.stop_sequences.or(self.stop_sequences),
            thinking: other.thinking.or(self.thinking),
        }
    }

    pub fn is_empty(&self) -> bool { /* all None */ }
}

// ProviderConfig gains a sampling field (domain.rs)
pub struct ProviderConfig {
    // ... existing fields ...
    pub sampling: Option<SamplingConfig>,  // NEW
}
// ProviderConfig::merge() extends to merge sampling blocks field-wise.

// Pipeline already has `defaults: ProviderConfig`.
// Add pipeline-level orthogonal sampling:
pub struct Pipeline {
    // ... existing fields ...
    pub sampling_defaults: Option<SamplingConfig>,  // NEW: from defaults.sampling
}

// Step gains a sampling field:
pub struct Step {
    // ... existing fields ...
    pub sampling: Option<SamplingConfig>,  // NEW
}

// ── InvokeOptions addition (runner/mod.rs) ────────────────────────────
pub struct InvokeOptions {
    // ... existing fields ...
    pub sampling: Option<SamplingConfig>,
}

// ── Resolution helper (executor/helpers/runner_resolution.rs) ─────────
pub fn resolve_step_sampling(session: &Session, step: &Step) -> Option<SamplingConfig> {
    let pipeline_default = session.pipeline.sampling_defaults.clone().unwrap_or_default();
    let provider_attached = session.pipeline.defaults.sampling.clone().unwrap_or_default();
    let step_override = step.sampling.clone().unwrap_or_default();

    let merged = pipeline_default
        .merge(provider_attached)
        .merge(step_override);

    if merged.is_empty() { None } else { Some(merged) }
}

// ── HttpRunner ChatRequest addition (runner/http.rs) ──────────────────
#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}
```

## Open Questions for Review

1. ~~`stop_sequences` merge vs replace~~ **Resolved**: replace semantics,
   consistent with every other sampling field and with ail's `tools:`
   override (§5). Authors setting `stop_sequences` at step level get the
   list they wrote — no implicit accumulation from inherited scopes.
   Spec §30.3.1 documents the rule and guides authors to put baseline
   safety stops at the provider scope. Replace is strictly more expressive
   than append (you can emulate append by copying; you cannot remove an
   inherited stop under append).

2. ~~`thinking` shape~~ **Resolved** (D7): decimal fraction `thinking: f64`
   in [0.0, 1.0]. YAML bool aliases: `true`→1.0, `false`→0.0. Each runner
   quantizes to its own supported granularity (ClaudeCliRunner: quartiles →
   `--effort`; HttpRunner: threshold at 0.5 → boolean; future API with
   `budget_tokens`: `thinking * PROVIDER_MAX_BUDGET`). Pipeline author's
   numeric intent survives runner granularity upgrades without edits.

3. ~~`max_tokens` vs `max_output_tokens` naming~~ **Resolved**: `max_tokens`.
   Shorter, more widely recognized, matches Anthropic's convention. Runners
   that target OpenAI's newer `max_completion_tokens` map the name internally.

4. ~~Validation strictness: allow both `temperature` and `top_p`?~~
   **Resolved**: allow both, no conflict check. AIL passes values through
   as-is. Spec §30.2.3 explains the interaction and steers authors toward
   using `temperature` alone for most cases. This is a documentation
   concern, not a validation concern — pipeline authors get to choose.

5. **Per-step `provider:` field wiring**: this design assumes a future
   `step.provider: <name>` field that resolves to a named provider profile.
   Until §15 aliases land, provider-scope sampling only works via the single
   `defaults.provider.sampling:` block — which applies to *every* step.
   That's fine for v1 (equivalent to pipeline-scope) and degrades gracefully
   when §15 lands.

6. **Ail's native runner (future)**: when we build it, this spec is the
   forward contract. The native runner implements all fields as the
   "platonic ideal" — temperature, top_p, top_k, max_tokens, stop_sequences,
   and thinking all behave per §30 without translation loss.

## Dependency Graph

```
Phase 1 (data model) ──┬── Phase 2 (executor) ──── Phase 3 (runners)
                       │                            Phase 4 (turn log)
                       └── Phase 5 (materialize)
Phase 6 (tests) depends on all above
Phase 7 (docs) can proceed in parallel
```

Phases 1→2→3 are the critical path. Phases 4, 5, 7 can be done in parallel
once Phase 1 is done.
