## 21. Planned Extensions

These features are designed and their syntax is reserved. Not yet implemented. Do not use in production pipelines — the current parser will reject them.

> **For contributors:** Open an issue referencing this section before beginning implementation work.

---

### Structured Step I/O Schemas

> Promoted to §26. See [s26-output-schema.md](s26-output-schema.md).

---

### Parallel Step Execution

> **Status: Design complete — see §29**

Parallel step execution has been fully designed and promoted to its own spec section. See [s29-parallel-execution.md](s29-parallel-execution.md) for the complete specification covering `async:`, `depends_on:`, `action: join`, the session fork model, structured join with `output_schema` namespacing, error handling, and turn log format.

#### Multi-Provider Parallel Sampling

> **Status: Planned — design seed (see D-020)**

A specific application of parallel execution (§29): the same prompt sent simultaneously to two providers. The canonical use case is quality comparison — a frontier model and a commodity model running in parallel, with a synthesis step that identifies what the frontier did better and proposes prompt improvements that would produce similar results from the commodity runner.

```yaml
# Uses §29 parallel primitives — async: true + depends_on:
- id: implement_frontier
  async: true
  prompt: "{{ step.invocation.prompt }}"
  provider: frontier

- id: implement_commodity
  async: true
  prompt: "{{ step.invocation.prompt }}"
  provider: commodity

- id: quality_compare
  depends_on: [implement_frontier, implement_commodity]
  action: join

- id: analysis
  prompt: |
    Two implementations of the same task:
    {{ step.quality_compare.response }}
    What did the frontier implementation do better? What prompt change would produce
    the frontier result from the commodity model?
  on_result:
    - always:
      action: pause_for_human
      message: "Quality comparison complete. Approve pipeline update?"
```

This creates a systematic feedback loop: the pipeline's prompts improve over time at directing any runner. Combined with the self-modifying pipeline primitive (see below), the comparison step can propose and apply those improvements automatically.

---

### Self-Modifying Pipelines

> **Status: Deferred — post-POC. Significant design work required.**
>
> Dependencies: stable pipeline execution (v0.0.1), parallel step execution (above), structured step I/O schemas (above), HITL approval flow (§13), hot reload mechanism (§22).

#### The Core Vision

Every pipeline run produces a structured evidence record in the turn log (§4.4). Over time this log accumulates: the prompt, the response, the tool calls, the `on_result` branch that fired, the step that handed control to human review. It is a machine-readable record of how the agent fails and how those failures were resolved.

The self-modifying pipeline reads that record and uses it. On approval, the diff is committed. The pipeline hot-reloads. The next invocation runs against an improved version of itself:

```yaml
# Note: log-injection and hot-reload require primitives described below.
# This is a design seed. Syntax is not final.

- id: pipeline_reflection
  context:
    run_log:
      last_n: 50          # inject the 50 most recent log entries
  prompt: |
    Review the attached pipeline run log.
    Identify the most common mismatch between intended and actual output.
    Propose a new step that would prevent it.
    Format your response as a YAML diff targeting the existing pipeline.
  on_result:
    always:
      action: pause_for_human
      message: "Pipeline improvement proposed. Approve to apply."
```

#### Why This Is Categorically Different

The self-modifying pipeline is not:

- **CLAUDE.md / memory files** — those are static context injection. `ail` steps are executable logic. A step that checks linter output fires every time regardless of context state. A memory file is a request; a pipeline step is a guarantee.
- **Hooks** — hooks cannot modify themselves and carry no HITL model.
- **Orchestration frameworks** — LangGraph, CrewAI, and similar tools manage multi-agent communication graphs. The self-modifying pipeline is a specific, bounded application of the pipeline model to its own improvement — not general orchestration.

The distinction that matters: the improvement is expressed in the same YAML that runs it, making it readable, diffable, testable, and version-controlled by anyone on the team. The accumulated operational knowledge of every developer who has ever committed to that repository lives in a file that any agent can run and any engineer can read.

#### The SWE-bench Validation

The hypothesis is runnable: a set of declared pipelines — linter step, test runner step, self-evaluation step — should improve a model's own SWE-bench score using that same model, with no changes to the weights. SWE-bench failures cluster around exactly the failure modes §1 names: output is produced but not verified against the test suite; there is no comparison circuit. A pipeline that guarantees the linter passes and the tests run before the score is recorded addresses this directly.

Either outcome is informative. A confirmed improvement validates the architectural claim. A null result prompts spec revision. The benchmark exists; the pipelines can be written.

> **Near-term path:** The SWE-bench experiment does not require the self-modifying pipeline or the plugin system below. It requires only v0.1 primitives: `context: shell:` steps, `on_result` branching on exit codes, and the `--headless` flag. An external driver script iterates over benchmark tasks, calling `ail --headless --once "<task>" --pipeline swe-bench.yaml` per task. See §20 (v0.1 scope) for the target pipeline.

#### Required Primitives (not yet specced)

**Log injection (`context: run_log:`)**
A way to reference the accumulated pipeline run log in a context step. The run log is already written to `~/.ail/projects/<sha1_of_cwd>/runs/` (§4.4). Log injection makes a filtered slice of that log available to a prompt step without a manual shell command. The exact syntax and filtering interface are not yet designed.

**Pipeline diff application (`action: apply_pipeline_diff`)**
A new `on_result` action that accepts a YAML diff from a preceding prompt step and applies it to the active pipeline file. The runtime must validate that the result is a valid pipeline before writing. Invalid diffs are treated as step errors escalating via `on_error`, not silent failures.

**Hot reload**
After `apply_pipeline_diff`, the runtime reloads the active pipeline. The next invocation uses the new version. The open question is timing — can hot reload happen mid-run (affecting later steps in the same execution), or only between runs? See §22.

**`FROM`-based layering for modifications**
Self-modifying pipelines should use `FROM` inheritance (§7) to layer improvements: the base pipeline remains unchanged, and each modification is an inheriting layer. This keeps changes auditable and reversible without overwriting the base. Full `FROM` traversal must be implemented before this is usable.

#### Relationship to §1 Scope Discipline

The third category in §1's scope compass — "extends `ail`'s capacity to select, compose, or improve its own pipelines" — is precisely what this section specifies. Norman and Shallice's Supervisory Attentional System intervenes when a task is novel or requires overriding a habitual response. Today `ail` is the contention scheduler: execute the declared pipeline. The self-modifying pipeline is where the SAS layer begins — `ail` improving the schema it schedules.

#### Relationship to Multi-Provider Quality Comparison

The multi-provider parallel sampling pattern (above) is a natural feeder for the self-modifying pipeline. The quality comparison step identifies what the frontier implementation did better; the self-modifying step proposes the prompt change that would close that gap. Run together, they form a closed feedback loop:

1. Two providers run the same prompt in parallel
2. A comparison step identifies the quality delta
3. The reflection step proposes a prompt improvement
4. Human approval applies the diff
5. The next invocation runs with tighter instructions

The gap between frontier and commodity closes because the pipeline's instructions get better at directing it — not because either model improves.

---

### Remote `FROM` Targets

> **Status: Planned**

Pipeline URIs, versioning, and a registry system analogous to Docker Hub. `FROM` currently accepts file paths only. URI support will be designed alongside tagging and version pinning.

---

### Step Output Visibility Control

> **Status: Planned — pending streaming research**

Per-step control over what the TUI displays: full streaming, final response only, or silent (pipeline-internal steps).

```yaml
- id: quality_score
  prompt: "Rate code quality 0.0–1.0. Respond with the number only."
  display: silent
```

---

### Dry Run Mode

> **Status: Implemented — v0.2**

The `--dry-run` flag executes the full pipeline resolution — template variable substitution, condition evaluation, step ordering — but replaces actual runner invocations with no-ops that return a synthetic `[DRY RUN] No LLM call made` response. No LLM API calls are made. Shell context steps (`context: shell:`) execute normally since they are local and free.

#### Usage

```bash
ail --dry-run "add a fizzbuzz function" --pipeline .ail.yaml
ail --dry-run --once "review code" --pipeline .ail.yaml
```

#### Behaviour

1. All template variables are resolved and displayed (using synthetic responses from earlier steps where needed).
2. Step execution order is shown with step IDs, types, and any condition/runner/model overrides.
3. `context: shell:` steps execute normally — their stdout, stderr, and exit codes are captured.
4. `prompt:` steps receive the resolved prompt but the runner returns a synthetic response instead of calling an LLM.
5. `on_result` branches are evaluated against the synthetic/shell responses.
6. Output is clearly labelled with `[DRY RUN]` prefix on every diagnostic line.

#### Implementation

- `DryRunRunner` (`ail-core/src/runner/dry_run.rs`) — a `Runner` implementation that returns a fixed synthetic response without spawning any subprocess or making any API call. Follows the same pattern as `StubRunner` but is intended for production use.
- The `--dry-run` CLI flag (`ail/src/cli.rs`) selects the `DryRunRunner` instead of the normal runner factory. All other pipeline infrastructure — config loading, session creation, template resolution, executor dispatch — runs identically to a normal invocation.
- Output formatting lives in `ail/src/dry_run.rs` in the binary crate.

---

### Direct MCP Tool Invocation

> **Status: Exploratory** — The concept and value are clear. The design has unresolved questions. Syntax is not yet reserved.

MCP (Model Context Protocol) is an open standard for connecting LLMs to external tools and data sources. An MCP server exposes named tools — functions that can be called with structured arguments to read data, query systems, or perform actions — and returns structured results.

There are two ways MCP can interact with `ail`:

**Mode 1 — LLM-driven (already implied by the step model)**  
During a pipeline step that calls an LLM directly, the LLM may emit tool calls targeting an MCP server. `ail` acts as the MCP client, executes the tool, and returns the result to the LLM. This is the standard MCP use case and requires no new pipeline syntax — it is a provider/runtime concern.

**Mode 2 — Pipeline-driven (this extension)**  
A pipeline step calls an MCP tool directly, bypassing the LLM entirely. The tool result flows forward as the step's response, available to subsequent steps via `{{ step.<id>.response }}`. No tokens are consumed.

```yaml
# Proposed syntax — not final
- id: get_repo_structure
  mcp:
    server: filesystem
    tool: list_directory
    arguments:
      path: "{{ session.cwd }}"

- id: analyse_structure
  prompt: |
    Here is the repository structure:
    {{ step.get_repo_structure.response }}
    Identify any architectural concerns.
```

This fits the Unix pipe philosophy already established in the spec: a deterministic, zero-token step that gathers data and passes it downstream.

**Unresolved questions before this can be specced:**

- How does `ail` discover and connect to MCP servers? Via a config block, a running process, or a URI?
- How are MCP servers declared — per-pipeline, per-session, or globally in `~/.config/ail/`?
- How is authentication handled for MCP servers that require credentials?
- If an MCP tool call fails, how does it interact with `on_error`? MCP errors are structured — can `on_result` match against them?
- Is `mcp:` a primary field (peer to `prompt:`, `skill:`, `pipeline:`) or a sub-field of `action:`?
- Should the pipeline be able to *register* MCP tools that the LLM can use in subsequent steps, or only call them directly?

**Why this matters:**  
Direct MCP invocation makes `ail` pipelines genuinely composable with the broader MCP ecosystem. A pipeline could gather live data from any MCP-compatible source — filesystem, database, web search, calendar, code analysis tools — before passing it to an LLM step, without burning tokens on the retrieval itself. This is particularly valuable for research pipelines and compliance workflows where the data gathering step must be deterministic and auditable.

---

### Native LLM Provider Support (OpenAI-Compatible REST)

> **Status: Planned — no CLI runner required**

A native runner tier that calls OpenAI-compatible `/v1/chat/completions` REST endpoints directly, without wrapping a CLI tool. Enables `ail` pipelines to run against any model host that exposes the OpenAI-compatible API — Ollama, Together AI, Groq, LiteLLM, corporate LLM proxies, and Anthropic's compatibility layer — without requiring a CLI agent to be installed.

```yaml
# Proposed syntax — not final
providers:
  local_ollama:
    type: openai-compatible
    base_url: http://localhost:11434/v1
    model: llama3.2
  hosted_groq:
    type: openai-compatible
    base_url: https://api.groq.com/openai/v1
    api_key_env: GROQ_API_KEY
    model: mixtral-8x7b-32768

pipeline:
  - id: fast_triage
    prompt: "Is this a security issue? Answer yes or no."
    provider: local_ollama
```

When a step uses a native REST provider, `ail` acts as the agent for that turn — managing conversation history, tool call dispatch, streaming reassembly, and context window state. This is a significant responsibility boundary shift from the CLI runner model (where the runner manages these concerns) and is called out explicitly in ARCHITECTURE.md §8.

The pipeline language syntax for `providers:` is forward-compatible with this extension. The `type:` field is reserved for this purpose and currently has no effect.

**Prerequisite:** native runner support depends on the `ail serve` server mode (see below) being designed first, as the two share the same HTTP client infrastructure and auth model.

---

### Safety Guardrails

> **Status: Exploratory** — The structural pattern is clear. Several design questions remain open, and one fundamental limitation must be stated plainly. Syntax is not yet reserved.

#### The Two Layers

Safety in `ail` operates across two distinct layers with very different reliability guarantees:

**Layer 1 — What `ail` can enforce deterministically**
Constraints on the pipeline itself: what gets injected into prompts, which skills and pipelines are permitted, which configurations are blocked at parse time. These are runtime guarantees — `ail` either allows the pipeline to run or it does not.

**Layer 2 — What depends on the underlying model**
Whether a model actually follows an injected instruction is a model concern, not a runtime concern. `ail` can inject "never include credentials in your response" into every prompt, but it cannot guarantee the model obeys it. For hard safety requirements, the reliable pattern is a dedicated validation step after any step that might produce sensitive content — using `input_schema` + `field:` + `equals:` for deterministic branching. A pipeline step is deterministic in a way that a model instruction is not.

This distinction must be understood by anyone relying on `ail` for safety-critical workflows.

#### Proposed Structure

Safety resources follow the same pattern as `observability:` — declared as a top-level block, inheritable via `FROM`, with org-declared directives carrying a `required: true` flag that child pipelines cannot override or remove.

```yaml
safety:

  # Injected into every prompt in this pipeline, unconditionally
  directives:
    - id: no_credentials
      inject: "Never include credentials, API keys, or passwords in your response."
      position: prepend         # or: append
      required: true            # child pipelines cannot disable this directive

    - id: pii_reminder
      inject: "Do not reproduce personally identifiable information."
      position: prepend
      required: true

  # Step configurations rejected at parse time
  blocklist:
    - pattern: "disable.*hitl"
      message: "HITL gates cannot be disabled by policy."

  # Only these skill and pipeline paths are permitted
  skill_allowlist:
    - ail/*
    - ./skills/*
    - /etc/ail/approved-skills/*

  pipeline_allowlist:
    - ./pipelines/*
    - /etc/ail/approved-pipelines/*
```

#### Inheritance via `FROM`

Safety directives declared in a `FROM` base pipeline are inherited by all child pipelines. A child may add its own directives but cannot remove or override a directive marked `required: true`. Any attempt to do so is a parse error, not a silent failure.

This makes safety directives a governance guarantee at the org level — the same model used for observability compliance.

#### The Reliable Safety Pattern

For any output that must be validated — not just instructed — a dedicated validation step provides stronger guarantees than a model directive alone. Where deterministic branching is required, `input_schema` + `field:` + `equals:` is the recommended approach:

```yaml
pipeline:
  - id: generate_code
    prompt: ./prompts/generate.md

  - id: safety_check
    prompt: |
      Review the above output strictly for the following:
      - No credentials, tokens, or secrets
      - No personally identifiable information
      - No hardcoded environment-specific values
      Answer only with SAFE or UNSAFE.
    input_schema:
      type: object
      properties:
        result:
          type: string
          enum: ["SAFE", "UNSAFE"]
      required: [result]
    on_result:
      field: result
      equals: "SAFE"
      if_true:
        action: continue
      if_false:
        action: pause_for_human
        message: "Safety violation detected. Review required before proceeding."
```

This is deterministic — the pipeline either continues or it does not — in a way that prompt injection alone cannot be.

#### Unresolved Questions

- Are `blocklist` patterns evaluated at parse time (static analysis of the YAML) or at runtime (when a step is about to execute)? Parse time is safer but may not catch dynamically constructed step configurations.
- How does the `skill_allowlist` interact with built-in `ail/*` modules? They should probably be implicitly permitted unless explicitly removed.
- Should `required: true` directives be visible to child pipeline authors — i.e. should `materialize` show injected directive content — or should they be opaque to prevent circumvention?
- Can a directive declare which step types it applies to (e.g. only `prompt:` steps, not `action:` steps), or does it apply universally?
- How does sensitive directive content interact with the observability layer? A safety directive that contains policy language may itself be sensitive and should not appear in trace exports by default.

---

### Model Benchmarking

> **Status: Exploratory** — The building blocks exist in the current spec. A dedicated benchmarking execution model requires design work that goes beyond the current single-run pipeline model. Syntax is not yet reserved.

#### What the Current Spec Already Supports

The `ail/model-compare` built-in and multi-provider routing cover the simplest benchmarking case — same prompt, two models, side-by-side output reviewed by a human. For casual model comparison this is sufficient and available today.

#### What a Real Benchmarking Workflow Needs

A serious benchmarking workflow requires capabilities the current spec does not express:

- **Dataset input** — run against a controlled set of prompts, not just a single human input
- **Repeatability** — run the same prompt N times against the same model to measure variance
- **Structured scoring** — a quality score per run captured as data, not just human review
- **Aggregation** — collect results across all runs into a comparable report
- **Isolation** — benchmark runs must not share context with production pipeline runs or with each other

None of these fit the current execution model, which is designed around a single `invocation` triggering a single pipeline run. Benchmarking is fundamentally a multi-run, dataset-driven execution model.

#### Proposed Direction

Benchmarking is a strong candidate for the plugin extensibility layer (see below). Rather than adding `benchmark:` as a core spec keyword, it would be expressed as an `x-benchmark:` extension that a benchmarking plugin handles:

```yaml
# Proposed — not final syntax
x-benchmark:
  dataset: ./benchmarks/security-prompts.jsonl
  runs_per_prompt: 5
  models:
    - openai/gpt-4o
    - anthropic/claude-opus-4-5
    - groq/llama-3.1-70b-versatile
  scoring:
    skill: ./skills/quality-scorer/
    output_schema: ./schemas/benchmark-score.json
  report:
    format: markdown
    destination: ./reports/{{ benchmark.run_id }}.md
```

The benchmark plugin would manage the multi-run execution loop, invoke the pipeline's steps for each input, collect structured scores, and produce a report — all without the core runtime needing to know about datasets or aggregation.

#### Why This Matters as a Vertical

For the LLM researcher segment, benchmarking is table-stakes. The ability to run a controlled dataset through multiple models, score the outputs with a custom skill, and produce a comparable report — entirely from a YAML file — is a genuinely novel capability that no current tool provides cleanly. It is also a natural lead-in to the learning loop use case: a benchmark run that identifies which model performs best on a given task type can feed directly into routing decisions.

#### Unresolved Questions

- Is a `trigger: dataset` the right execution model, or should benchmarking be entirely plugin-managed outside the normal pipeline trigger system?
- How does context isolation work between benchmark runs? Each run must be completely independent.
- Should scoring be a skill (LLM-evaluated) or a deterministic function (regex, JSON schema validation, exact match)? Both are useful; the spec should support both.
- How are benchmark results stored and compared across runs over time — is this a persistence concern for the `ail` runtime, or for the plugin?

---

### Plugin Extensibility Layer

> **Status: Exploratory** — The `x-` prefix model is the leading candidate. Core runtime changes are minimal. Full plugin dispatch mechanism requires design. Syntax partially reserved (`x-` prefix).

#### Motivation

As `ail` grows, vertical use cases will emerge that are too specific to belong in the core spec — benchmarking, Datadog integration, custom compliance frameworks, team-specific reporting. Adding each as a core keyword pollutes the spec and creates a maintenance burden. A plugin extensibility layer allows third parties to extend the YAML language itself, adding new top-level keywords and new step fields, without forking the spec or the runtime.

#### The Leading Candidate: `x-` Prefix Model

Following the Docker Compose convention, any top-level key or step field prefixed with `x-` is reserved for extensions. The core runtime passes `x-` fields to registered plugin handlers. If no handler is registered for a given `x-` field, it is either silently ignored or raises a warning — configurable by policy.

```yaml
# Third-party plugins extend the top-level namespace
x-datadog:
  api_key: "{{ env.DD_API_KEY }}"
  service: "ail-pipelines"
  env: production

x-benchmark:
  dataset: ./benchmarks/prompts.jsonl
  runs_per_prompt: 3

# Plugins can also add step-level fields
pipeline:
  - id: security_audit
    prompt: ./prompts/security.md
    x-notify:
      channel: "#security-alerts"
      on: pause_for_human
```

#### Plugin Registration

Plugins are declared in `~/.config/ail/plugins.yaml` or in a `plugins:` block in the pipeline file:

```yaml
# Proposed — not final syntax
plugins:
  - id: datadog
    path: ~/.ail/plugins/datadog/
    handles: [x-datadog]

  - id: benchmark
    path: ~/.ail/plugins/benchmark/
    handles: [x-benchmark]
    trigger: manual            # benchmark plugin registers its own trigger type
```

A plugin is itself an Agent Skills-compatible directory — a `PLUGIN.md` file declaring its capabilities, accepted `x-` fields, and handler entry point. This keeps the plugin format consistent with the skill format already established in the spec.

#### What a Plugin Can Do

A plugin handler receives the full parsed pipeline and the values of its declared `x-` fields. It may:

- Add steps to the pipeline before execution begins
- Register new trigger types
- Subscribe to step lifecycle events (before step, after step, on error)
- Write to the audit trail
- Export to external systems

A plugin may not modify core pipeline behaviour — step execution order, `on_result` logic, HITL gate behaviour — without those modifications being visible in `materialize` output.

#### Governance and Safety Interaction

Plugins declared in a `FROM` base pipeline are inherited by child pipelines. A base pipeline may declare a plugin as `required: true`, preventing child pipelines from removing it — the same governance model used for safety directives and observability resources.

```yaml
plugins:
  - id: compliance-reporter
    path: /etc/ail/plugins/compliance-reporter/
    handles: [x-compliance]
    required: true             # child pipelines cannot remove this plugin
```

#### Unresolved Questions

- Should the plugin entry point be a compiled binary, a script, or a WASM module? Each has different portability and security implications. WASM is the most sandboxed but has the highest implementation cost.
- How does `materialize` represent plugin-injected steps? They must be visible to maintain the "no surprises" guarantee.
- Can a plugin declare new `on_result` action types, or are those reserved for the core spec?
- Should plugins be sandboxed from the filesystem and network by default, with explicit capability grants? Given that plugins run as part of a pipeline that may handle sensitive data, this seems important but adds implementation complexity.
- Is `PLUGIN.md` the right format, or should plugins have a more structured manifest (JSON schema, TOML) given that they declare machine-readable capabilities rather than human-readable instructions?

---

### Pipeline Registry & Versioning

> **Status: Planned**

Named pipeline identity, versioning, and a registry system. Enables `FROM: org/security-base@2.1` style references. Will be designed alongside remote `FROM` support.

---

### Observability — Tracing & Logging

> **Status: Exploratory** — The structure and value are clear. Several design questions remain open. Syntax is not yet reserved.

#### Motivation

`ail` pipelines are multi-step, multi-provider, potentially multi-pipeline executions. Without structured observability, debugging a misbehaving pipeline means reading raw stdout. For teams with compliance requirements, there is no auditable record of what ran, when, against what input, and what it produced.

The 15-factor app standard already mandates that the runtime emits structured logs to stdout. This extension goes further: it allows pipelines to declare named observability resources — traces and loggers — and reference them from individual steps, in the same way Docker Compose declares `networks:` and `volumes:` as top-level resources that services reference by name.

#### Proposed Structure

Observability resources are declared in a top-level `observability:` block and referenced from steps. This keeps configuration centralised and step definitions clean.

```yaml
observability:

  traces:
    - id: main_trace
      exporter: otlp
      endpoint: "{{ env.OTEL_ENDPOINT }}"
      service_name: "ail/{{ pipeline.run_id }}"

  logs:
    - id: audit_log
      format: json
      destination: stdout          # 15-factor compliant; the default for all logs

    - id: step_detail_log
      format: json
      destination: file
      path: ./.ail/logs/{{ pipeline.run_id }}.jsonl

defaults:
  trace: main_trace                # applied to every step unless overridden
  log: [audit_log]

pipeline:
  - id: security_audit
    prompt: "..."
    log: [audit_log, step_detail_log]   # step-level override adds a destination

  - id: internal_score
    prompt: "Rate quality 0.0–1.0. Number only."
    trace: none                         # opt this step out of tracing
```

#### OpenTelemetry Compatibility

OTEL is the target standard for trace export. Each pipeline run maps to an OTEL trace; each step maps to a span. Span attributes would carry at minimum:

- `ail.step.id`
- `ail.step.provider`
- `ail.step.token_usage.input`
- `ail.step.token_usage.output`
- `ail.step.condition.result`
- `ail.step.on_result.matched`
- `ail.pipeline.run_id`
- `ail.pipeline.file`

This makes `ail` pipeline executions visible in any OTEL-compatible backend — Grafana, Jaeger, Honeycomb, Datadog — with zero additional integration work.

The pipeline run log (§4.4) is the authoritative source; the OTEL exporter is a consumer of it.

#### Inheritance via `FROM`

Observability resources declared in a `FROM` base pipeline are inherited by all child pipelines. An organisation can declare a mandatory `compliance_trace` in its base pipeline and all inheriting pipelines emit to it automatically. A child pipeline may add additional loggers but cannot silently remove an inherited tracer — any attempt to `disable` a base-declared observability resource creates an explicit audit event rather than silently succeeding.

This makes compliance observability a governance guarantee, not a convention.

#### Unresolved Questions

- Are inherited observability resources merged (child adds to parent's list) or overridable (child can replace parent's config)? Merging is safer for compliance; overriding is more flexible for development.
- Can a step opt out of a default tracer? The `trace: none` syntax above is proposed but not decided. There may be compliance contexts where opting out should be impossible.
- How does sensitive data interact with trace export? Prompt content may contain credentials, PII, or proprietary information. There should be a way to mark prompt content as redacted in trace spans without disabling tracing entirely. This needs explicit design.
- Is the `observability:` block itself inheritable via `FROM` hook operations (`run_before`, `run_after`, `override`, `disable`), or does it follow a simpler merge model separate from the step hook system?
- What is the minimum viable observability for v0.0.1? Almost certainly structured JSON to stdout — already implied by 15-factor compliance. Everything else in this section is additive on top of that baseline.

---

### Template Variable Fallbacks

> **Status: Planned**

The `default()` filter in v0.1 accepts string literals only:
```yaml
{{ env.VAR_NAME | default("") }}
{{ env.VAR_NAME | default("NOT_SET") }}
```

A future version will support file references and inline blocks for cases where the fallback is a full prompt, markdown document, or complex JSON object:
```yaml
# File reference — fallback loaded from disk at resolution time
{{ env.SYSTEM_PROMPT | default(file: ./defaults/fallback-prompt.md) }}
{{ step.optional_context.response | default(file: ./defaults/empty-context.json) }}

# Inline block — for structured content without a separate file
{{ env.CONFIG | default(inline: '{"mode": "safe", "timeout": 120}') }}
```

This is particularly relevant for optional context injection steps where a missing variable should produce a well-formed prompt or configuration object rather than an empty string or short token. Until this extension is implemented, declare required variables explicitly and handle optional configuration via dedicated steps with `condition:` guards.

---
