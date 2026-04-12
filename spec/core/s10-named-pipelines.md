## 10. Named Pipelines & Composition

> **Status: Implemented — v0.2.**

Multiple named pipelines can be defined within a single `.ail.yaml` file using the
top-level `pipelines:` map. Each key is a pipeline name; each value is an ordered list
of steps using the same schema as the main `pipeline:` array.

Named pipelines are referenced from `pipeline:` step bodies by name (rather than by
file path). At parse time, if a `pipeline:` value matches a key in the `pipelines:`
map, the step is resolved as a named pipeline reference (`StepBody::NamedPipeline`).
Otherwise it is treated as a file path (`StepBody::SubPipeline`).

### Syntax

```yaml
version: "0.0.1"

pipelines:
  security_gates:
    - id: vuln_scan
      prompt: "Identify vulnerabilities."
    - id: license_check
      prompt: "Check license compliance."

  quality_check:
    - id: lint
      prompt: "Run linting."

pipeline:
  - id: main_work
    prompt: "Do the main work."
  - id: run_security
    pipeline: security_gates
  - id: run_quality
    pipeline: quality_check
```

### Execution Model

Named pipeline steps execute in isolation, following the same model as file-based
sub-pipelines (§9):

1. A fresh child `Session` is created with the named pipeline's steps.
2. The child session's invocation prompt defaults to the parent's most recent response,
   or can be overridden with an explicit `prompt:` field on the step.
3. The child session inherits the parent pipeline's `named_pipelines` map, so nested
   named pipeline references work.
4. The child session inherits `defaults` (provider config, tool policy) from the parent.
5. The child's final step response becomes the calling step's `response` in the turn log.
6. Named pipeline steps increment the sub-pipeline depth counter — the same
   `MAX_SUB_PIPELINE_DEPTH` (16) guard applies.

### Prompt Override

A `prompt:` field on a named pipeline step overrides the child session's invocation
prompt, identical to the §9 sub-pipeline prompt override:

```yaml
pipeline:
  - id: review_code
    pipeline: security_gates
    prompt: "Review the following code for security issues: {{ step.invocation.response }}"
```

### Circular Reference Detection

Circular references among named pipelines are detected at **validation time** (during
`config::load()`), not at runtime. The validator performs a DFS cycle check on the
named pipeline dependency graph. If a cycle is found, a
`PIPELINE_CIRCULAR_REFERENCE` error is returned.

The `materialize --expand-pipelines` command also performs independent cycle detection
during expansion.

### Template Variables

Named pipeline step responses are accessible via the standard template syntax:

- `{{ step.<id>.response }}` — the named pipeline's final step response

### Validation Rules

1. Named pipeline names must be non-empty strings.
2. Each named pipeline must contain at least one step.
3. Steps within named pipelines follow the same validation rules as main pipeline steps
   (unique IDs within the named pipeline, exactly one primary field, etc.).
4. Circular references among named pipelines produce a `PIPELINE_CIRCULAR_REFERENCE` error.

---
