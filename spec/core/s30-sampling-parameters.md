# 30. Sampling Parameter Control

> **Status**: draft — design proposal for #120

## 30.1 Purpose

Different pipeline steps have fundamentally different generation needs.
A code-writing step benefits from low temperature (deterministic, precise),
while a brainstorming step benefits from high temperature (creative, diverse).
Extended thinking matters for complex reasoning steps but wastes tokens on simple ones.

This section specifies how pipeline authors control LLM sampling parameters
at the pipeline level (defaults) and per step (overrides).

## 30.2 The `sampling:` Block

A `sampling:` block may appear in two places:

1. **Pipeline defaults** — `defaults.sampling:` sets baseline parameters for all steps.
2. **Per step** — `sampling:` on any `prompt:` or `skill:` step overrides the pipeline default.

Context steps (`context: shell:`) and action steps (`action:`) bypass the runner
entirely, so `sampling:` is ignored on those step types (validation warning).

### 30.2.1 Supported Fields

| Field | Type | Range | Description |
|---|---|---|---|
| `temperature` | float | 0.0–2.0 | Controls randomness. 0.0 = deterministic, higher = more creative. |
| `top_p` | float | 0.0–1.0 | Nucleus sampling — consider tokens comprising the top P probability mass. |
| `top_k` | integer | ≥ 1 | Consider only the top K most likely tokens. Not supported by all providers. |
| `max_tokens` | integer | ≥ 1 | Maximum number of tokens in the response. |
| `stop_sequences` | list of strings | — | Stop generation when any of these strings is produced. |
| `thinking` | boolean | — | Enable/disable extended thinking (chain-of-thought). |

All fields are optional. Absent fields inherit from the pipeline default;
if absent there too, the runner's own default applies.

### 30.2.2 Syntax

```yaml
# Pipeline-level defaults
defaults:
  model: claude-sonnet-4-20250514
  sampling:
    temperature: 0.3
    max_tokens: 4096

pipeline:
  # Inherits defaults: temperature=0.3, max_tokens=4096
  - id: analyze
    prompt: "Analyze the codebase for security issues"
    sampling:
      thinking: true          # override: enable thinking for this step

  # Full override
  - id: brainstorm
    prompt: "Brainstorm 10 creative solutions"
    sampling:
      temperature: 0.9
      top_p: 0.95
      max_tokens: 8192

  # No sampling block — uses pipeline defaults as-is
  - id: summarize
    prompt: "Summarize the above findings"
```

## 30.3 Merge Semantics

Sampling parameters merge at the **field level**, not block level.
A step's `sampling:` block overrides individual fields from the pipeline default;
unspecified fields fall through.

```
effective = pipeline.defaults.sampling.merge(step.sampling)
```

Where `merge(other)` applies: `other.field.unwrap_or(self.field)` per field.

This is identical to the `ProviderConfig.merge()` semantics already used for
model/provider resolution (§15).

**Merge chain**: `defaults.sampling` → step `sampling:` → (no CLI override for sampling).

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

The Claude CLI does not currently expose sampling parameter flags.
The ClaudeCliRunner therefore **warns on all sampling parameters** in the
initial implementation. When Claude CLI adds `--temperature`, `--max-tokens`,
or similar flags, the runner will map them.

> **Implementation note**: The `--model` flag already flows through. If Anthropic
> adds sampling flags to the CLI, adding support is a one-line change in
> `build_subprocess_spec()`.

### 30.4.3 HTTP Runner (`http`, `ollama`)

The HTTP runner controls the API request body directly and supports:

| Sampling field | Maps to | Notes |
|---|---|---|
| `temperature` | `"temperature"` in request body | |
| `top_p` | `"top_p"` in request body | |
| `top_k` | `"top_k"` in request body | Anthropic API; ignored by OpenAI-compat if unsupported |
| `max_tokens` | `"max_tokens"` in request body | |
| `stop_sequences` | `"stop"` in request body | OpenAI format; Anthropic uses `"stop_sequences"` |
| `thinking` | `"think"` in request body | Existing `HttpRunnerConfig.think` field; sampling overrides it |

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

Sampling parameters are orthogonal to provider strings. The provider string
selects *which model*; sampling controls *how the model generates*.

```yaml
- id: creative_step
  model: anthropic/claude-sonnet-4-20250514
  sampling:
    temperature: 0.9
```

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
      thinking: true
```

This keeps the sampling spec simple and orthogonal.

## 30.6 Validation

### 30.6.1 Parse-time

- `temperature` must be in [0.0, 2.0] if present.
- `top_p` must be in [0.0, 1.0] if present.
- `top_k` must be ≥ 1 if present.
- `max_tokens` must be ≥ 1 if present.
- `stop_sequences` must be a non-empty list of non-empty strings if present.
- `thinking` must be a boolean if present.
- `sampling:` on a `context:` or `action:` step emits a validation warning.

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
