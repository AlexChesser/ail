## 26. Structured Step I/O Schemas

> **Implementation status:** Fully implemented. `output_schema` — parse-time JSON Schema validation, runtime response validation, `{{ step.<id>.items }}` template variable. `input_schema` — parse-time validation, runtime preceding-step-output validation, parse-time schema compatibility checks between adjacent steps (§26.3). `field:` + `equals:` operator (§26.4) — binary branching format for `on_result` with `if_true`/`if_false` actions, parse-time field-in-schema validation. Schema-as-file-path (§26.1) is not yet implemented.

The `on_result` prose-matching operators (`contains`, `matches`, `starts_with`) are documented as best-effort in §5.4 — LLMs do not deterministically produce exact tokens. Structured I/O schemas provide a reliable alternative: a step declares what structured output it produces or expects, the runtime validates the JSON, and `on_result` can branch on known fields with exact equality.

---

### 26.1 `output_schema`

A step may declare an `output_schema` — a JSON Schema describing the structure it promises to produce. The runtime validates the step's response against this schema after execution. A validation failure is treated as a step error and escalates via `on_error`.

`output_schema` is optional. It has no effect on how the prompt is sent to the provider unless the provider natively supports schema-constrained output (see §26.7).

```yaml
- id: classify
  prompt: "Classify the task. Respond with JSON only."
  output_schema:
    type: object
    properties:
      category:
        type: string
        enum: ["bugfix", "feature", "refactor"]
      priority:
        type: integer
        minimum: 1
        maximum: 5
    required: [category, priority]
```

#### Schema as file path

Both `output_schema` and `input_schema` accept either an inline YAML block or a file path. Path detection follows the same rules as `prompt:` file paths (`./`, `../`, `~/`, `/`). File-referenced schemas are loaded at parse time.

```yaml
# Inline block
output_schema:
  type: array
  items:
    type: string

# File path — both sides can reference the same schema file
- id: plan
  output_schema: ./schemas/task-list.json

- id: implement_tasks
  for_each:
    over: "{{ step.plan.items }}"
  input_schema: ./schemas/task-list.json   # same file, no duplication
```

#### JSON Schema compliance

`output_schema` and `input_schema` accept standard JSON Schema. The `$schema` field inside the schema block (inline or file) determines which draft `ail` uses for validation. If `$schema` is absent, `ail` defaults to Draft 7.

This means schemas written for OpenAI's structured output, Anthropic tool definitions, or any other JSON Schema–based tooling are directly reusable. No AIL-specific schema format is defined. Binary schema formats (Avro, Protocol Buffers) are not supported.

---

### 26.2 `input_schema`

A step may declare an `input_schema` — a JSON Schema describing the structure it expects from the preceding step's output. The runtime validates the preceding step's response against this schema before the step executes. A validation failure is treated as a step error and escalates via `on_error`.

`input_schema` is independent of `output_schema`. Both may be declared on the same step.

```yaml
- id: route
  prompt: "Handle the {{ step.classify.category }} task."
  input_schema:
    type: object
    properties:
      category:
        type: string
        enum: ["bugfix", "feature", "refactor"]
    required: [category]
  on_result:
    field: category
    equals: "bugfix"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Non-bugfix task. Route manually."
```

---

### 26.3 Parse-Time Schema Compatibility

When adjacent steps declare `output_schema` + `input_schema`, the runtime validates schema compatibility at **parse time** — before any execution begins. A mismatch between a declared output schema and the following step's declared input schema is a parse error (`SCHEMA_COMPATIBILITY_FAILED`), not a runtime error.

Parse-time compatibility checks catch interface mismatches between pipeline steps during pipeline load, not mid-run.

> **Limitation:** The schema-as-file-path variant (§26.1) is not yet implemented. Parse-time compatibility checks cannot run on schemas declared as file paths because the schema file is not loaded during pipeline validation in v0.3. This caveat applies only to the file-path variant; inline schemas are fully validated at parse time.

---

### 26.4 The `field:` + `equals:` Operator in `on_result`

When a step declares `input_schema`, `on_result` gains access to the `field:` + `equals:` operator — exact equality matching against a named field in the validated JSON:

```yaml
on_result:
  field: category
  equals: "bugfix"
  if_true:
    action: continue
  if_false:
    action: pause_for_human
```

**Parse-time rule:** A step that declares `on_result` using `field:` + `equals:` **must** also declare an `input_schema` that includes the referenced field. This is enforced at parse time.

This operator stops short of a general expression language. Field access plus exact equality covers the reliable branching use case without reopening the expression language question. For more complex conditions, use the condition expression language (§12.2).

---

### 26.5 Accessing Array Output

When a step's `output_schema` declares `type: array`, the array is available to subsequent steps and to `for_each:` (§28) via `{{ step.<id>.items }}`.

```yaml
- id: plan
  prompt: "Break this feature into implementation tasks. Respond with a JSON array of strings."
  output_schema:
    type: array
    items:
      type: string
    maxItems: 20

- id: implement_tasks
  for_each:
    over: "{{ step.plan.items }}"
    as: task
  steps:
    - id: implement
      prompt: "Implement: {{ for_each.task }}"
      resume: true
```

`{{ step.<id>.items }}` is only available for steps that declared `output_schema` with `type: array`. Referencing `.items` on a step without an array schema is a `TEMPLATE_UNRESOLVED` error.

---

### 26.6 Full Example

```yaml
- id: classify
  prompt: "Classify the task. Respond with JSON only."
  output_schema:
    type: object
    properties:
      category:
        type: string
        enum: ["bugfix", "feature", "refactor"]
      priority:
        type: integer
        minimum: 1
        maximum: 5
    required: [category, priority]

- id: handle
  prompt: "Handle the {{ step.classify.category }} task (priority {{ step.classify.priority }})."
  input_schema:
    type: object
    properties:
      category:
        type: string
        enum: ["bugfix", "feature", "refactor"]
      priority:
        type: integer
    required: [category]
  on_result:
    field: category
    equals: "bugfix"
    if_true:
      action: continue
    if_false:
      action: pause_for_human
      message: "Non-bugfix. Route manually."
```

---

### 26.7 Provider Compatibility

When a step declares `output_schema`, `ail` passes the schema to the provider for **constrained decoding** — a mechanism where token sampling is filtered so the model is mechanically constrained to produce output matching the schema. With constrained decoding active, the model cannot produce markdown code fences, extra prose, or any output that doesn't conform to the schema.

**Implementation status:** Implemented for `HttpRunner`. `ClaudeCliRunner` and `CodexRunner` ignore `output_schema` in `InvokeOptions` — validation is ail-side only for those runners.

#### HttpRunner paths

| Runner name | Parameter | Format |
|---|---|---|
| `ollama` | `format` (top-level body field) | JSON Schema object passed directly |
| `http` | `response_format` | `{ type: "json_schema", json_schema: { name: "output", schema: <schema>, strict: true } }` |

The `ollama_compat` flag on `HttpRunnerConfig` controls which path is used. It is set automatically by `HttpRunner::ollama()` and by `RunnerFactory` when the runner name is `"ollama"`.

Providers that do not support constrained decoding receive the request without schema constraints; `ail` validates the response after receipt. A validation failure is treated as a step error and escalates via `on_error`.

#### Prompt description not required

When constrained decoding is active, the model does not need to be told the output format in the prompt — the schema IS the format instruction to the provider. Prompts can focus on the task content only.

---

### 26.8 Validation Rules

1. `output_schema` and `input_schema` must be valid JSON Schema (inline block or file path). An invalid schema is a `CONFIG_VALIDATION_FAILED` error at parse time.
2. File-path schemas that cannot be read are a `CONFIG_FILE_NOT_FOUND` error.
3. When adjacent steps declare `output_schema` and `input_schema`, schema compatibility is validated at parse time. Incompatible schemas → `SCHEMA_COMPATIBILITY_FAILED`.
4. `on_result: field: + equals:` requires a declared `input_schema` containing the referenced field. Missing `input_schema` or missing field → `CONFIG_VALIDATION_FAILED`.
5. `{{ step.<id>.items }}` requires `output_schema` with `type: array` on step `<id>`. No schema or non-array schema → `TEMPLATE_UNRESOLVED`.

---

### 26.9 Error Types

| Constant | Value | When produced |
|---|---|---|
| `OUTPUT_SCHEMA_VALIDATION_FAILED` | `ail:schema/output-validation-failed` | Step output failed `output_schema` validation at runtime |
| `INPUT_SCHEMA_VALIDATION_FAILED` | `ail:schema/input-validation-failed` | Prior step output failed `input_schema` validation before step execution |
| `SCHEMA_COMPATIBILITY_FAILED` | `ail:schema/compatibility-failed` | Adjacent `output_schema` / `input_schema` are incompatible at parse time |

---
