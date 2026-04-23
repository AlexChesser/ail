# AIL Specification — Compact Reference

> **ail** — Artificial Intelligence Loops. YAML-orchestrated post-prompt pipeline runtime.
> This is a compressed reference. For full prose: `ail spec --format prose`.

## §1 Purpose

ail is a **control plane** for LLM agent behavior. After each human prompt, ail fires a declared sequence of automated steps before returning control. Steps run in declared order; individual steps may be skipped or exit early via declared outcomes.

**Core guarantee (§4.2):** Once `execute()` begins, all declared steps run in order. Early exit only via explicit declared outcomes — never silent failures.

## §2 Vocabulary

| Term | Definition |
|---|---|
| pipeline | Ordered sequence of steps in `.ail.yaml` |
| step | Single unit: prompt, skill, context, sub-pipeline, do_while, for_each, or action |
| invocation | Implicit first step — the human's triggering prompt |
| session | One running instance of an underlying agent |
| runner | Adapter calling the underlying agent (`claude`, `http`, `ollama`, plugin) |
| turn log | Append-only NDJSON audit trail per run |
| passthrough | No `.ail.yaml` found — ail is transparent |

## §3 File Format

**Discovery order** (first match wins): `--pipeline <path>` > `.ail.yaml` in CWD > `.ail/default.yaml` in CWD > `~/.config/ail/default.yaml` > passthrough.

**Top-level fields:** `version` (req, string `"1"`), `FROM` (opt, path for inheritance §7), `defaults` (opt), `pipeline` (req, step array), `pipelines` (opt, named pipeline map §10), `meta` (opt, informational).

**defaults:** `model`, `timeout_seconds`, `max_concurrency` (for async steps), `provider` (model, base_url, auth_token, sampling), `tools` ({allow, deny, disabled}), `sampling` (temperature, top_p, top_k, max_tokens, stop_sequences, thinking).

## §4 Execution Model

1. Discover and load pipeline (or passthrough)
2. If no `invocation` step declared: run user prompt via `runner.invoke()`, record `TurnEntry(step_id="invocation")`
3. Call `executor::execute()` for all declared steps in order
4. Steps run isolated by default; `resume: true` continues prior session

**Run log:** NDJSON at `~/.ail/projects/<sha1_cwd>/runs/<run_id>.jsonl`. Every step produces a `TurnEntry`.

## §5 Step Types

Six body types. Exactly one primary field per step. `id` is always required.

| Type | Field | LLM call | Token cost |
|---|---|---|---|
| Prompt | `prompt:` | Yes | Yes |
| Skill | `skill:` | Yes | Yes |
| Context | `context:` | No | No |
| Sub-pipeline | `pipeline:` | Delegated | Delegated |
| Do-while | `do_while:` | Delegated | Delegated |
| For-each | `for_each:` | Delegated | Delegated |

### Common fields (all step types)

`id` (req, string), `runner` (opt), `condition` (opt, §12), `on_error` (opt, `continue`|`retry`|`abort_pipeline`, dflt `abort_pipeline`), `max_retries` (opt, dflt 2), `disabled` (opt, bool), `resume` (opt, bool, dflt false), `model` (opt), `tools` (opt, {allow, deny, disabled}), `system_prompt` (opt, string|filepath), `append_system_prompt` (opt, [{text|file|shell|spec}]), `on_result` (opt), `output_schema` (opt, JSON Schema), `input_schema` (opt, JSON Schema), `sampling` (opt), `async` (opt, bool), `depends_on` (opt, [step_id]), `before` (opt, [chain_step]), `then` (opt, [chain_step]).

### prompt: steps
Inline text or file path (prefix `./`, `../`, `~/`, `/`). Supports template vars. File paths resolved relative to pipeline file.

### skill: steps
Path to directory containing SKILL.md, or `ail/<builtin>`. Built-ins: `ail/code_review`, `ail/test_writer`, `ail/security_audit`, `ail/janitor`.

### context: steps
`context: { shell: "<command>" }` — runs `/bin/sh -c`, captures stdout/stderr/exit_code. No LLM. Non-zero exit = result, not error.
`context: { spec: "<query>" }` — injects embedded AIL spec content. Query: `compact`|`schema`|`prose`|section ID (`s05`,`r02`).

### pipeline: steps
Path to another `.ail.yaml`. Runs in isolated child session. Depth guard = 16. `prompt:` on same step overrides child invocation.

### action: steps
`pause_for_human` — HITL gate. `modify_output` — human edits step output. `join` — sync barrier for parallel deps. `reload_self` — hot-reload pipeline YAML.

### Runner selection hierarchy
Per-step `runner:` > `AIL_DEFAULT_RUNNER` env > `"claude"`.

## §7 Pipeline Inheritance (FROM)

`FROM: ./base.yaml` — inherit steps. Hook operations: `run_before: <id>`, `run_after: <id>`, `override: <id>`, `disable: <id>`. Chains must be acyclic.

## §9 Sub-Pipelines

Child session is isolated (fresh turn log). Failure in child propagates to parent. Depth guard: 16 levels. Template vars in `pipeline:` path resolved at execution time. `prompt:` field overrides child invocation prompt.

## §10 Named Pipelines

`pipelines:` section defines reusable step lists by name. Referenced via `pipeline: <name>`. Same isolation model as file-based sub-pipelines. Circular refs detected at parse time.

## §11 Template Variables

`{{ variable }}` syntax. Resolved at step execution time. **Unresolved = fatal error, never empty string.** Can only reference already-executed steps (no forward refs). Skipped step refs = parse error.

| Variable | Value |
|---|---|
| `{{ step.invocation.prompt }}` | Original user prompt |
| `{{ step.invocation.response }}` | Response before pipeline steps |
| `{{ last_response }}` | Most recent step response |
| `{{ step.<id>.response }}` | Named prompt/skill step response |
| `{{ step.<id>.result }}` | Context step output (stdout+stderr for shell) |
| `{{ step.<id>.stdout }}` | Shell context stdout |
| `{{ step.<id>.stderr }}` | Shell context stderr |
| `{{ step.<id>.exit_code }}` | Shell context exit code (string) |
| `{{ step.<id>.items }}` | Array from output_schema type: array |
| `{{ step.<id>.tool_calls }}` | Tool events as JSON array |
| `{{ step.<id>.modified }}` | modify_output gate result |
| `{{ pipeline.run_id }}` | UUID for this run |
| `{{ session.tool }}` | Runner name |
| `{{ session.cwd }}` | Working directory |
| `{{ env.VAR }}` | Env var (fatal if unset) |
| `{{ env.VAR \| default("x") }}` | Env var with fallback |
| `{{ do_while.iteration }}` | Loop: 0-based iteration index |
| `{{ do_while.max_iterations }}` | Loop: declared max |
| `{{ for_each.item }}` | Iteration: current item |
| `{{ for_each.<as> }}` | Iteration: item under `as:` name |
| `{{ for_each.index }}` | Iteration: 1-based index |
| `{{ for_each.total }}` | Iteration: total items |
| `{{ step.<loop>::<step>.response }}` | Inner step from loop (final iteration) |
| `{{ step.<join>.<dep>.response }}` | Structured join: dep output |

## §12 Conditions

`condition:` field on any step. Values: `always` (default), `never`, or expression string.

**Expression operators:** `==`, `!=`, `contains`, `starts_with`, `ends_with`, `matches /PAT/FLAGS`. LHS is template var, RHS is literal. Regex flags: `i` (case), `m` (multiline), `s` (dotall).

## §13 HITL Gates

`action: pause_for_human` — blocks until human approves. `action: modify_output` — human edits prior step output; result in `{{ step.<id>.modified }}`. `on_headless:` controls behavior in `--once`/headless: `skip` (dflt), `abort`, `use_default`.

## §16 Error Handling

`on_error:` per step. `abort_pipeline` (dflt) — stop immediately. `continue` — log error, proceed. `retry` — retry up to `max_retries` (dflt 2) then abort. Non-zero shell exits are results, not errors (trigger `on_result`, not `on_error`).

## §19 Runners

Three-tier model: `ClaudeCliRunner` (reference), `HttpRunner` (OpenAI-compatible API), plugin runners (JSON-RPC over stdin/stdout).

`RunnerFactory::build(name, headless)` resolves by name. Factory names: `claude`, `http`, `ollama`, or plugin discovered from `~/.ail/runners/`.

## §26 Output/Input Schema

`output_schema:` — JSON Schema validated against step response at runtime. Enables `{{ step.<id>.items }}` for array schemas. `input_schema:` — validated against preceding step output before step executes. `field:` + `equals:` in `on_result` requires `input_schema`.

## §27 do_while

Bounded repeat-until loop. `max_iterations` (req, >= 1), `exit_when` (req, §12 condition), `steps` or `pipeline` (req, mutually exclusive). Inner step IDs namespaced: `<loop_id>::<step_id>`. Shared depth guard with for_each (MAX_LOOP_DEPTH=8).

## §28 for_each

Collection iteration over validated array. `over` (req, template ref to `.items`), `as` (opt, dflt `item`), `max_items` (opt), `on_max_items` (opt, `continue`|`abort_pipeline`), `steps` or `pipeline` (req). Same depth guard as do_while.

## §29 Parallel Execution

`async: true` marks non-blocking step. `depends_on: [ids]` creates sync barrier. `action: join` merges outputs. String join: concatenated with `[id]:` headers. Structured join: if all deps have output_schema, merges into `{ dep_id: output }`. `on_error: fail_fast` (dflt) or `wait_for_all`. No forward refs, no cycles, no concurrent resume conflict.

## §30 Sampling Parameters

Three-scope merge: `defaults.sampling` < `defaults.provider.sampling` < step `sampling:`. Field-level merge; `stop_sequences` replaces (not appends). `thinking:` accepts float [0,1] or bool (true→1.0, false→0.0). Runner quantization: Claude CLI → `--effort` quartiles; HTTP → bool threshold 0.5.

## §31 Spec Access

Spec is compiled into the binary. Access via:
- CLI: `ail spec`, `ail spec --format compact`, `ail spec --section s05`
- Pipeline context: `context: { spec: compact }`, `context: { spec: s05 }`
- System prompt: `append_system_prompt: [{ spec: compact }]`
- Tiers: `schema` (~2K tokens, YAML structure), `compact` (this document, ~10K tokens), `prose` (full spec, ~80K tokens)
