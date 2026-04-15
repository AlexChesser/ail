# AIL Specification — Navigation Index

> **ail** — Alexander's Impressive Loops
> *The executive function layer for LLM agents.*

---

## Why This Is Split

The full spec (~2200 lines) and runner spec (~250 lines) are split into per-section files to keep LLM context costs low. When working with `ail`, you rarely need the entire spec — you need one or two sections at a time. Find the section below, read only that file.

To reassemble the full spec as a single document:

```bash
cat spec/core/s*.md        # full pipeline language spec
cat spec/runner/r*.md      # full runner contract spec
```

---

## Core Spec — `spec/core/`

The AIL Pipeline Language Specification — for pipeline authors and implementers.

| File | Section | One-line summary | Status |
|---|---|---|---|
| [s01-purpose.md](core/s01-purpose.md) | §1 Purpose & Philosophy | Cognitive science grounding; core guarantee; scope discipline; two-layer model | **alpha** |
| [s02-vocabulary.md](core/s02-vocabulary.md) | §2 Concepts & Vocabulary | Term definitions — pipeline, step, invocation, skill, context, etc. | reference |
| [s03-file-format.md](core/s03-file-format.md) | §3 File Format | 4-step discovery order (§3.1); top-level YAML schema | partial — §3.1 discovery ✓; `defaults.model`/`defaults.provider`/`defaults.timeout_seconds`/`defaults.tools` ✓; timeout parsed not enforced; `FROM`/`meta`/`providers` not parsed |
| [s04-execution-model.md](core/s04-execution-model.md) | §4 Execution Model | invocation step (§4.1); core guarantee (§4.2); §4.4 run log + NDJSON events; §4.5 controlled execution mode | partial — §4.1–§4.2 + §4.4 run log ✓; §4.5 `execute_with_control()`, `ExecutionControl`, `ExecutorEvent`, NDJSON stdin protocol ✓; §4.3 hooks/conditions/on_result not impl |
| [s05-step-specification.md](core/s05-step-specification.md) | §5 Step Specification | Four step types (prompt/skill/context/pipeline); `skill:` replaces prompt for self-contained invocations; context sources: shell/mcp; on_result; append_system_prompt; tools; then/before | **alpha** — `id`/`prompt`/`tools`/`context:`/`on_result`/`pipeline:`/`system_prompt:`/`append_system_prompt:`/`resume:` impl; `before:`/`then:`/`skill:` (stub) not yet impl |
| [s06-skills.md](core/s06-skills.md) | §6 Skills | SKILL.md format (open standard fields); `skill:` step type; `$ARGUMENTS` substitution; REPL `/skill-name` discovery; Agent Skills compatibility | **alpha** — not yet impl |
| [s07-pipeline-inheritance.md](core/s07-pipeline-inheritance.md) | §7 Pipeline Inheritance | FROM; hook operations (run_before/run_after/override/disable) | **alpha** — FROM path resolution, cycle detection, hook operations, defaults merging impl |
| [s08-hook-ordering.md](core/s08-hook-ordering.md) | §8 Hook Ordering | Onion model; discovery order governs hook precedence | **alpha** — onion model within FROM chain impl; multi-layer discovery merge deferred |
| [s09-calling-pipelines.md](core/s09-calling-pipelines.md) | §9 Calling Pipelines as Steps | Sub-pipeline isolation; failure propagation | **alpha** — sub-pipeline isolation, failure propagation, and depth guards implemented |
| [s10-named-pipelines.md](core/s10-named-pipelines.md) | §10 Named Pipelines | Multiple named pipelines in one file — define, reference, execute, circular detection | **v0.2** |
| [s11-template-variables.md](core/s11-template-variables.md) | §11 Template Variables | `{{ }}` syntax; all variable paths incl. `{{ step.<id>.result }}` for context steps | **alpha** — all template variables implemented including `step.<id>.result`/`stdout`/`stderr`/`exit_code`, env vars, session vars |
| [s12-conditions.md](core/s12-conditions.md) | §12 Conditions | `condition:` field; named conditions (if_code_changed, etc.); §12.3 regex syntax (shared with `on_result: matches:` / `expression:`) | partial — `never`/`always` implemented |
| [s13-hitl-gates.md](core/s13-hitl-gates.md) | §13 HITL Gates | pause_for_human; tool permission flow diagram | partial — `pause_for_human` implemented in `execute_with_control()` (TUI/JSON mode); no-op in simple `execute()` mode |
| [s14-built-in-modules.md](core/s14-built-in-modules.md) | §14 Built-in Modules | ail/janitor, ail/security-audit, ail/test-writer, etc. | deferred |
| [s15-providers.md](core/s15-providers.md) | §15 Providers | Provider strings; aliases; `resume:` for session continuity | partial — `defaults.model`/`defaults.provider` ✓; per-step `model:` ✓; per-step `resume:` ✓; provider string format/aliases deferred |
| [s16-error-handling.md](core/s16-error-handling.md) | §16 Error Handling | on_error: continue / pause_for_human / abort_pipeline / retry | deferred |
| [s17-materialize.md](core/s17-materialize.md) | §17 materialize | CLI command; output format with origin comments; `--expand-pipelines` | partial — single-file flatten + origin comments ✓; `--expand-pipelines` for named pipelines ✓; `FROM` chain traversal not impl |
| [s18-complete-examples.md](core/s18-complete-examples.md) | §18 Complete Examples | Full worked YAML — simplest, solo dev, org base, multi-speed | needs update for new step types |
| [s19-runners-adapters.md](core/s19-runners-adapters.md) | §19 Runners & Adapters | Three-tier runner model; RunnerFactory; per-step dispatch; plugin runner system | **v0.2** — RunnerFactory, per-step dispatch, plugin discovery + JSON-RPC protocol ✓ |
| [s20-mvp.md](core/s20-mvp.md) | §20 MVP v0.0.1 Scope | What is and isn't in scope for v0.0.1 | reference — v0.0.1 complete; alpha scope is next |
| [s21-planned-extensions.md](core/s21-planned-extensions.md) | §21 Planned Extensions | Multi-provider quality comparison (D-020), self-modifying pipelines (D-019), MCP, plugins, observability; **Dry Run Mode implemented v0.2**; parallel execution moved to §29 | partial |
| [s22-open-questions.md](core/s22-open-questions.md) | §22 Open Questions | Unresolved design questions (completion detection, hot reload, self-modifying pipeline approval/validation, etc.) | reference |
| [s23-structured-output.md](core/s23-structured-output.md) | §23 Structured Output | `--output-format json` NDJSON event stream; event schema; ordering guarantees | **v0.1** ✓ |
| [s24-log-command.md](core/s24-log-command.md) | §24–25 The `ail log` and `ail logs` Commands | §24: single-run inspection; `--format` and `--follow` flags; exit codes; project scoping. §25: multi-session listing; `--session`, `--query`, `--tail`, `--limit`, `--format`; FTS search; JSON output schema | **alpha** — both commands fully documented |
| [s26-output-schema.md](core/s26-output-schema.md) | §26 Structured Step I/O Schemas | `output_schema` / `input_schema`; JSON Schema compliance (`$schema` field selects draft, defaults to Draft 7); file-path or inline block; parse-time compatibility check; `field:` + `equals:` in `on_result`; array access via `{{ step.<id>.items }}`; provider compatibility | **draft** |
| [s27-do-while.md](core/s27-do-while.md) | §27 `do_while:` — Bounded Repeat-Until | Bounded generate→test→fix loop; `max_iterations` (required), `exit_when` (§12.2 syntax), `on_max_iterations`; step namespacing (`<loop_id>::<step_id>`); iteration scope; turn log events; executor events | **draft** |
| [s28-for-each.md](core/s28-for-each.md) | §28 `for_each:` — Collection Iteration | Map steps over a validated array from a prior `output_schema: type: array` step; `over`, `as`, `max_items`, `on_max_items`; plan-execution pattern; requires §26 | **v0.3** |
| [s29-parallel-execution.md](core/s29-parallel-execution.md) | §29 Parallel Step Execution | `async: true`, `depends_on:`, `action: join`; session fork model; structured join with `output_schema` namespacing; `on_error: fail_fast \| wait_for_all`; cancel signals; template scoping rules; turn log concurrent_group | **v0.3** — parallel dispatch, session forking, string+structured joins, all validation rules, 23 tests |
| [s30-sampling-parameters.md](core/s30-sampling-parameters.md) | §30 Sampling Parameter Control | `sampling:` block at three scopes (pipeline / provider-attached / per-step); temperature, top_p, top_k, max_tokens, stop_sequences, thinking; field-level merge; stop_sequences replace semantics; runner-specific quantization of `thinking` (ClaudeCLI `--effort`, HTTP boolean, ail-native passthrough) | **v0.3** — spec + all runners + tests |

---

## Runner Spec — `spec/runner/`

The AIL Runner Contract — for CLI tool authors and adapter writers.

| File | Section | One-line summary | Status |
|---|---|---|---|
| [r01-overview.md](runner/r01-overview.md) | Purpose, Background, Compliance Tiers | What `ail` needs from any runner; minimum vs. extended compliance | reference |
| [r02-claude-cli.md](runner/r02-claude-cli.md) | Reference Implementation — Claude CLI | Verified flags, event stream, session continuity, tool permission interface | v0.1 ✓ — invocation, session-continuity, `--allowedTools`, MCP bridge HITL all implemented and validated |
| [r03-targets.md](runner/r03-targets.md) | Known Runners, Custom Adapters, Open Questions | Roadmap runners; Runner trait for adapters; remaining open questions | partial — `http`/`ollama` implemented |
| [r04-ail-log-format.md](runner/r04-ail-log-format.md) | r04. AIL Log Format Specification | Terminal-safe markdown+directives format; version header, thinking/tool-call/tool-result/stdio directives, turns, costs, errors | **alpha** |
| [r05-http-runner.md](runner/r05-http-runner.md) | r05. HTTP Runner — Direct OpenAI-Compatible API | Direct API runner for Ollama and any OpenAI-compatible endpoint; session continuity, config, tool policy, error mapping | **v0.1** ✓ |
| [r10-plugin-protocol.md](runner/r10-plugin-protocol.md) | r10. AIL Runner Plugin Protocol | JSON-RPC 2.0 over stdin/stdout; initialize/invoke/shutdown lifecycle; streaming notifications; permission flow | **alpha** |
| [r11-plugin-discovery.md](runner/r11-plugin-discovery.md) | r11. Runner Plugin Discovery | Manifest format; `~/.ail/runners/` directory; executable resolution; runner name rules; factory integration | **alpha** |
