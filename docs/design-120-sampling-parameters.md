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

### D2: YAML syntax — `sampling:` block

**Decision**: Nested `sampling:` block on both `defaults:` and per-step.

```yaml
defaults:
  sampling:
    temperature: 0.3
pipeline:
  - id: creative
    prompt: "..."
    sampling:
      temperature: 0.9
```

**Rationale**: Groups sampling params cleanly. Parallels `tools:`, `on_result:`,
`context:` as a nested block. Keeps the step's top-level namespace for
behavioral fields (id, prompt, model, runner, condition, etc.).

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

### D7: `thinking` subsumes HttpRunner's `think` field

**Decision**: `sampling.thinking: bool` replaces the current
`HttpRunnerConfig.think` as the per-step control. The config-level `think`
remains as the runner's default.

**Rationale**: The HttpRunner already has `think: Option<bool>`. Promoting
this to a sampling parameter makes it accessible to pipeline authors
without knowing runner internals. The merge is: `sampling.thinking`
overrides `HttpRunnerConfig.think` for that step.

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
   - Add `SamplingDto` struct with all six fields (all `Option`)
   - Add `sampling: Option<SamplingDto>` to `StepDto`
   - Add `sampling: Option<SamplingDto>` to `DefaultsDto`

2. **`ail-core/src/config/domain.rs`**
   - Add `SamplingConfig` struct (Debug, Clone, Default) with all six fields
   - Add `SamplingConfig::merge(self, other) -> SamplingConfig` method
   - Add `sampling: Option<SamplingConfig>` to `Step`
   - Add `sampling_defaults: Option<SamplingConfig>` to `Pipeline`

3. **`ail-core/src/config/validation/mod.rs`**
   - Add `validate_sampling(dto: Option<SamplingDto>) -> Result<Option<SamplingConfig>>`
   - Range checks: temperature [0.0, 2.0], top_p [0.0, 1.0], top_k ≥ 1, max_tokens ≥ 1
   - Warn if `sampling:` appears on context/action step
   - Wire into `validate_steps()` step construction
   - Wire into pipeline-level validation for `defaults.sampling`

### Phase 2: Executor plumbing

**Files to change**:

4. **`ail-core/src/runner/mod.rs`**
   - Add `sampling: Option<SamplingConfig>` to `InvokeOptions`
   - Update `InvokeOptions::default()` (already None)

5. **`ail-core/src/executor/helpers/runner_resolution.rs`**
   - Add `resolve_step_sampling(session: &Session, step: &Step) -> Option<SamplingConfig>`
   - Merge: `session.pipeline.sampling_defaults.merge(step.sampling)`

6. **`ail-core/src/executor/dispatch/prompt.rs`**
   - Call `resolve_step_sampling()` and set `options.sampling`

7. **`ail-core/src/executor/dispatch/skill.rs`**
   - Same as prompt.rs — skill steps also invoke the runner

### Phase 3: Runner implementations

**Files to change**:

8. **`ail-core/src/runner/claude/mod.rs`**
   - In `build_subprocess_spec()`: if `options.sampling` has any `Some` field,
     emit `tracing::warn!("ClaudeCliRunner: sampling parameter '{name}' is not \
     supported by the claude CLI; ignoring")`
   - No CLI args added (claude CLI doesn't support them)

9. **`ail-core/src/runner/http.rs`**
   - Add sampling fields to `ChatRequest`: `temperature`, `top_p`, `top_k`,
     `max_tokens`, `stop` (mapped from `stop_sequences`)
   - In `invoke()`: read from `options.sampling`, populate `ChatRequest` fields
   - `sampling.thinking` overrides `self.config.think` for that invocation
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
#[derive(Debug, Default, Deserialize)]
pub struct SamplingDto {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u64>,
    pub max_tokens: Option<u64>,
    pub stop_sequences: Option<Vec<String>>,
    pub thinking: Option<bool>,
}

// ── Domain (domain.rs) ────────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct SamplingConfig {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u64>,
    pub max_tokens: Option<u64>,
    pub stop_sequences: Option<Vec<String>>,
    pub thinking: Option<bool>,
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

    /// Returns true if all fields are None.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.top_k.is_none()
            && self.max_tokens.is_none()
            && self.stop_sequences.is_none()
            && self.thinking.is_none()
    }
}

// ── InvokeOptions addition (runner/mod.rs) ────────────────────────────
pub struct InvokeOptions {
    // ... existing fields ...
    pub sampling: Option<SamplingConfig>,
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

1. **Should `stop_sequences` merge or replace?** Current design: step-level
   `stop_sequences` *replaces* (not appends to) pipeline default. This is
   simpler and avoids surprising accumulation. But append could be useful.
   **Proposed**: replace (matches how `tools:` works — step overrides pipeline).

2. **Should `thinking` support a `budget_tokens` variant?** The Anthropic API
   allows `thinking: { type: "enabled", budget_tokens: 10000 }`. We could model
   this as `thinking: true` (boolean) or `thinking: { budget_tokens: 10000 }`
   (struct). **Proposed**: boolean for v1, struct variant for later. The YAML
   syntax `thinking: true` is forwards-compatible — a future `thinking:
   budget_tokens: 10000` syntax doesn't conflict.

3. **`max_tokens` vs `max_output_tokens`?** Anthropic API uses `max_tokens`,
   OpenAI recently switched to `max_completion_tokens`. We use `max_tokens`
   because it's shorter and more widely recognized. Runners map to the
   provider's preferred name.

4. **Validation strictness**: Should we error or warn when `temperature` and
   `top_p` are both set? Some providers recommend using one or the other.
   **Proposed**: allow both — let the provider decide. No ail-level conflict
   check.

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
