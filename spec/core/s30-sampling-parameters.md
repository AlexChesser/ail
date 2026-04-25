## 30. Sampling Parameter Control

> **Implementation status:** v0.3 — fully implemented. Applies to `ClaudeCliRunner`, `HttpRunner`, and `StubRunner`. All validation rules and runner mappings are active at parse time and runtime.

## 30.1 Purpose

Different pipeline steps have fundamentally different generation needs.
A code-writing step benefits from low temperature (deterministic, precise),
while a brainstorming step benefits from high temperature (creative, diverse).
Extended thinking matters for complex reasoning steps but wastes tokens on simple ones.

This section specifies how pipeline authors control LLM sampling parameters
at the pipeline level (defaults) and per step (overrides).

## 30.2 The `sampling:` Block

`sampling:` is a **reusable block** that can be declared at three scopes.
It uses the same schema everywhere; only where it attaches changes:

1. **Pipeline defaults** — `defaults.sampling:` sets a baseline for every step.
2. **Provider-attached** — `defaults.provider.sampling:` (single provider) or
   `providers.<name>.sampling:` (named provider, per §15) binds sampling to a
   provider profile. When a step uses that provider, those sampling defaults
   apply — so "switch provider, switch behavior" is a single-knob change.
3. **Per step** — `sampling:` on any `prompt:` or `skill:` step overrides
   both pipeline and provider defaults for that step only.

Context steps (`context: shell:`) and action steps (`action:`) bypass the runner
entirely, so `sampling:` is ignored on those step types (validation warning).

**Design rationale for the reusable block**: pairing sampling with a provider
profile means the author defines `creative` and `precise` profiles once, and
steps just reference them. This avoids copying temperature/top_p across
every step, and keeps sampling intent co-located with the model it was
tuned for.

### 30.2.1 Supported Fields

| Field | Type | Range | Description |
|---|---|---|---|
| `temperature` | float | 0.0–2.0 | Controls randomness. 0.0 = deterministic, higher = more creative. |
| `top_p` | float | 0.0–1.0 | Nucleus sampling — consider tokens comprising the top P probability mass. |
| `top_k` | integer | ≥ 1 | Consider only the top K most likely tokens. Not supported by all providers. |
| `max_tokens` | integer | ≥ 1 | Maximum number of tokens in the response. |
| `stop_sequences` | list of strings | — | Stop generation when any of these strings is produced. |
| `thinking` | float | 0.0–1.0 | Reasoning / extended-thinking intensity as a fraction. `0.0` = off; `1.0` = maximum. Each runner quantizes to whatever granularity it supports. |

All fields are optional. Absent fields inherit from the next-higher scope;
if absent at every scope, the runner's own default applies.

> **YAML aliases for `thinking`**: boolean values are accepted as shorthand.
> `thinking: true` normalizes to `1.0`; `thinking: false` normalizes to `0.0`.
> This keeps simple on/off pipelines readable while allowing fine control
> when needed.
>
> **Rationale for a decimal**: runners have wildly different granularity —
> Ollama accepts a boolean, Claude CLI accepts a 4-level enum (`--effort`),
> and Anthropic's API is heading toward continuous `budget_tokens`. A single
> decimal value lets authors express intent once and each runner quantizes
> on its own terms. Upgrades in any runner's granularity happen without any
> pipeline change.

### 30.2.2 Syntax

**Scope 1 + 3: Pipeline defaults + per-step override**

```yaml
defaults:
  model: claude-sonnet-4-5
  sampling:                    # pipeline-wide baseline
    temperature: 0.3
    max_tokens: 4096

pipeline:
  # Inherits defaults: temperature=0.3, max_tokens=4096
  - id: summarize
    prompt: "Summarize the findings"

  # Per-step override (field-level merge)
  - id: brainstorm
    prompt: "Brainstorm 10 creative solutions"
    sampling:
      temperature: 0.9         # overrides 0.3; max_tokens still 4096
```

**Scope 2: Provider-attached sampling**

Sampling attaches to the provider block. When the step uses that provider,
those sampling defaults apply.

```yaml
defaults:
  provider:
    model: anthropic/claude-sonnet-4-5
    base_url: "https://api.anthropic.com/v1"
    sampling:                  # provider-attached — applies whenever this provider is used
      temperature: 0.2
      thinking: 0.75

pipeline:
  - id: review
    prompt: "Review the code"
    # Inherits provider.sampling: temperature=0.2, thinking=high
```

**Scope 2 (advanced): Named provider profiles** *(requires §15 aliases, deferred)*

```yaml
providers:
  creative:
    model: anthropic/claude-sonnet-4-5
    sampling:
      temperature: 0.9
      top_p: 0.95
      thinking: 0.2          # light reasoning
  precise:
    model: anthropic/claude-sonnet-4-5
    sampling:
      temperature: 0.0
      thinking: 1.0          # full reasoning

pipeline:
  - id: ideas
    provider: creative         # inherits creative.sampling
    prompt: "Generate 10 ideas"

  - id: implement
    provider: precise          # inherits precise.sampling
    prompt: "Implement the best idea"
    sampling:
      max_tokens: 16384        # step-level fine-tune on top
```

### 30.2.3 Picking Between `temperature` and `top_p`

Both `temperature` and `top_p` shape the token probability distribution
before sampling, and both are legal to set together. However, Anthropic and
OpenAI both recommend altering **one or the other, not both**. AIL
intentionally does **not** enforce this — the value is passed through to
the provider, which will accept both without error — but pipeline authors
should understand the interaction.

**What each does**

- **`temperature`** reshapes the full distribution over tokens. `0.0` is
  deterministic (argmax). Values below `1.0` sharpen the distribution
  (the model becomes more confident and less diverse); values above `1.0`
  flatten it (more randomness, more unusual tokens).
- **`top_p`** truncates the distribution: keep the smallest set of tokens
  whose cumulative probability reaches `p`, renormalize, then sample.
  `1.0` means no truncation; lower values narrow the candidate pool.

**How they compose**

Providers apply temperature first, then top_p truncation, then sample.
The more restrictive parameter tends to dominate:

- `temperature: 0.0` + any `top_p` → argmax wins; `top_p` is irrelevant.
- Sharp `temperature` (e.g. `0.3`) + loose `top_p` (e.g. `0.9`) →
  temperature dominates; the distribution is already concentrated.
- Flat `temperature` (e.g. `1.5`) + tight `top_p` (e.g. `0.1`) →
  `top_p` dominates; only a few tokens are eligible, sampled flatly among them.

**Practical guidance**

- For most pipelines, set only `temperature`. It's the more intuitive knob.
- Use `top_p` alone when you want to cap the long tail of improbable tokens
  without sharpening the whole distribution (e.g. creative writing that
  should stay coherent).
- Setting both is not wrong; it is simply harder to reason about. AIL
  respects the author's choice and ships the values to the provider as-is.

## 30.3 Merge Semantics

Sampling parameters merge at the **field level**, not block level.
Higher-precedence scopes override individual fields; unspecified fields
fall through from the lower scope.

**Merge chain** (right-hand wins, per field):

```
effective = defaults.sampling
    .merge(provider.sampling)    // provider-attached (if step uses a provider)
    .merge(step.sampling)        // per-step override
```

Where `merge(other)` applies `other.field.or(self.field)` per field —
identical to `ProviderConfig.merge()` semantics (§15).

**Precedence (low → high):**

1. `defaults.sampling` — pipeline baseline
2. `defaults.provider.sampling` (or `providers.<name>.sampling` when §15 lands)
3. `step.sampling` — per-step override

### 30.3.1 `stop_sequences` Replaces, Does Not Append

`stop_sequences` is a list, but it follows the same replace semantics as
every other field: a higher-precedence scope that sets `stop_sequences`
**replaces** the entire list from lower scopes. It does not append.

```yaml
defaults:
  provider:
    sampling:
      stop_sequences: ["Human:"]       # safety boundary

pipeline:
  - id: structured
    sampling:
      stop_sequences: ["</answer>"]    # REPLACES — "Human:" is gone
```

To keep inherited stops while adding new ones, include them explicitly:

```yaml
  - id: structured
    sampling:
      stop_sequences: ["Human:", "</answer>"]
```

**Rationale**: consistency with every other sampling field and with ail's
`tools:` override (§5). Replace is strictly more expressive than append
(you can emulate append by copying; you cannot remove an inherited stop
under append semantics). Authors who need `stop_sequences` are generally
operating at a level of detail where explicit lists aid clarity over
implicit accumulation.

**Guidance**: put critical safety stops (e.g., `"Human:"`) at the
provider-scope `sampling:` block, and — when writing a step that overrides
`stop_sequences` — remember to re-include the ones you still need.

There is intentionally no `--temperature` CLI flag. Sampling is a pipeline
design concern, not a runtime override. The `--model` CLI flag remains the
appropriate runtime knob for switching model behavior.

## 30.4 Runner Responsibilities

### 30.4.1 Core Contract

When `InvokeOptions.sampling` is `Some(config)`:

1. The runner MUST apply any parameters it supports.
2. The runner MUST emit a `tracing::warn!` for parameters it does not support.
   The warning includes the parameter name and the runner name.
3. The runner MUST NOT error on unsupported parameters — warnings only.

This ensures pipelines are portable across runners with graceful degradation.

### 30.4.2 Claude CLI Runner (`claude`)

The Claude CLI exposes **only one sampling-related flag**: `--effort
<low|medium|high|max>` (reasoning intensity). It does NOT expose
`--temperature`, `--top-p`, `--top-k`, `--max-tokens`, or `--stop-sequences`.

| Sampling field | ClaudeCliRunner behavior |
|---|---|
| `thinking: 0.0` | Omit `--effort` entirely (CLI default applies). |
| `thinking: (0.0, 0.25]` | `--effort low` |
| `thinking: (0.25, 0.50]` | `--effort medium` |
| `thinking: (0.50, 0.75]` | `--effort high` |
| `thinking: (0.75, 1.0]` | `--effort max` |
| `temperature` | Warn: "not supported by claude CLI; ignored" |
| `top_p` | Warn: same |
| `top_k` | Warn: same |
| `max_tokens` | Warn: same (distinct from `--max-budget-usd`, which is a dollar cap) |
| `stop_sequences` | Warn: same |

The quartile quantization is the default mapping; it MAY be refined if
Anthropic adds more `--effort` levels in future.

When Anthropic adds sampling flags to the CLI, each is a one-line change in
`build_subprocess_spec()`. The spec is the forward contract; runner support
fills in over time.

> **On `max_tokens` vs `--max-budget-usd`**: Claude CLI has no per-response
> token cap, but does have a dollar budget cap. These are different knobs
> and we do NOT auto-map one to the other.

### 30.4.3 HTTP Runner (`http`, `ollama`)

The HTTP runner controls the API request body directly and supports:

| Sampling field | Maps to | Notes |
|---|---|---|
| `temperature` | `"temperature"` in request body | |
| `top_p` | `"top_p"` in request body | |
| `top_k` | `"top_k"` in request body | Anthropic API; ignored by OpenAI-compat if unsupported |
| `max_tokens` | `"max_tokens"` in request body | |
| `stop_sequences` | `"stop"` in request body | OpenAI format; Anthropic uses `"stop_sequences"` |
| `thinking: < 0.5` | `"think": false` | |
| `thinking: >= 0.5` | `"think": true` | Intensity is collapsed to a boolean. When a provider adds `budget_tokens` support, the runner will map `thinking` to `budget_tokens = thinking * PROVIDER_MAX_BUDGET` instead. |

### 30.4.4 Plugin Runners (JSON-RPC)

The `invoke` request includes a `sampling` object in `params`:

```json
{
  "jsonrpc": "2.0",
  "method": "invoke",
  "id": 1,
  "params": {
    "prompt": "...",
    "model": "...",
    "sampling": {
      "temperature": 0.9,
      "max_tokens": 4096
    }
  }
}
```

Plugin runners apply or ignore fields at their discretion. No error for
unrecognized fields.

## 30.5 Interaction with Other Features

### 30.5.1 Provider Strings (§15)

A `sampling:` block attaches to a provider. The provider string still selects
*which model*, and the attached sampling controls *how the model generates*.
When §15 named provider aliases land, each named provider carries its own
sampling profile:

```yaml
providers:
  creative:
    model: anthropic/claude-sonnet-4-5
    sampling: { temperature: 0.9, top_p: 0.95 }
  precise:
    model: anthropic/claude-sonnet-4-5
    sampling: { temperature: 0.0, thinking: 1.0 }

pipeline:
  - id: idea
    provider: creative
    prompt: "..."
  - id: ship
    provider: precise
    prompt: "..."
```

Switching `provider:` on a step switches model *and* sampling as one unit —
keeping intent co-located.

### 30.5.2 Sub-pipelines (§9)

Sub-pipelines inherit their parent's `defaults.sampling` only if the child
pipeline does not declare its own `defaults.sampling`. This follows the
existing sub-pipeline isolation rule — child sessions are independent.

### 30.5.3 Pipeline Inheritance / FROM (§7)

`defaults.sampling` participates in FROM inheritance with the same merge
semantics as `defaults.model`. The child's `defaults.sampling` overrides the
parent's at the field level.

### 30.5.4 Loops — `do_while:` (§27) and `for_each:` (§28)

Steps inside loop bodies use `sampling:` normally. The sampling config is
resolved per invocation, not cached for the loop.

### 30.5.5 Best-of-N Sampling

Best-of-N (generate N candidates, select the best) is NOT a built-in primitive.
It composes naturally from existing features:

```yaml
pipeline:
  - id: setup
    context:
      shell: "echo '[1,2,3]'"
    output_schema:
      type: array
      items: { type: integer }

  - id: candidates
    for_each:
      over: "{{ step.setup.items }}"
      steps:
        - id: generate
          prompt: "Propose a solution for: {{ step.invocation.prompt }}"
          sampling:
            temperature: 0.9

  - id: select
    prompt: |
      Pick the best solution from the candidates above.
      Explain why it's best.
    sampling:
      temperature: 0.0
      thinking: 1.0
```

This keeps the sampling spec simple and orthogonal.

## 30.6 Validation

### 30.6.1 Parse-time

- `temperature` must be in [0.0, 2.0] if present.
- `top_p` must be in [0.0, 1.0] if present.
- `top_k` must be ≥ 1 if present.
- `max_tokens` must be ≥ 1 if present.
- `stop_sequences` must be a non-empty list of non-empty strings if present.
- `thinking` must be a float in [0.0, 1.0] if present. YAML `true` normalizes to `1.0` and `false` to `0.0`. Values outside the range fail validation.
- `sampling:` on a `context:` or `action:` step emits a validation warning.
- The same schema validates at every scope (defaults / provider / step).

### 30.6.2 Runtime

No runtime validation beyond what the runner enforces. If the provider
rejects a value (e.g., temperature=2.0 on a provider that caps at 1.0),
the runner's error propagates as `ail:runner/invocation-failed`.

## 30.7 Materialize Output

`ail materialize` renders `sampling:` blocks with `# origin` comments:

```yaml
- id: brainstorm
  prompt: "Generate ideas"
  sampling:                    # origin: .ail.yaml:12
    temperature: 0.9           # origin: .ail.yaml:13
    max_tokens: 8192           # origin: .ail.yaml:14
```

## 30.8 Turn Log

Sampling parameters are recorded in the `TurnEntry` for observability:

```json
{
  "step_id": "brainstorm",
  "sampling": {
    "temperature": 0.9,
    "max_tokens": 8192
  },
  ...
}
```

Only non-default (explicitly set) fields appear. If no sampling was
configured, the `"sampling"` field is omitted.
