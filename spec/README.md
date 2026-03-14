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
| [s02-vocabulary.md](core/s02-vocabulary.md) | §2 Concepts & Vocabulary | Term definitions — pipeline, step, invocation, skill, etc. | reference — needs update for `context:` step type |
| [s03-file-format.md](core/s03-file-format.md) | §3 File Format | 4-step discovery order (§3.1); top-level YAML schema | partial — §3.1 discovery ✓; `FROM`/`meta`/`providers`/`defaults` not parsed |
| [s04-execution-model.md](core/s04-execution-model.md) | §4 Execution Model | invocation step (§4.1); core guarantee (§4.2); §4.4 run log + NDJSON events | partial — §4.1–§4.2 + §4.4 run log ✓; §4.3 hooks/conditions/on_result not impl |
| [s05-step-specification.md](core/s05-step-specification.md) | §5 Step Specification | Four step types (prompt/skill/context/pipeline); `skill:` replaces prompt for self-contained invocations; context sources: shell/mcp; on_result; append_system_prompt; tools; then/before | **alpha** — `id`/`prompt`/`tools` impl in v0.0.1; `skill:`/`context:`/`append_system_prompt:`/`on_result`/`before`/`then` not yet impl |
| [s06-skills.md](core/s06-skills.md) | §6 Skills | SKILL.md format (open standard fields); `skill:` step type; `$ARGUMENTS` substitution; REPL `/skill-name` discovery; Agent Skills compatibility | **alpha** — not yet impl |
| [s07-pipeline-inheritance.md](core/s07-pipeline-inheritance.md) | §7 Pipeline Inheritance | FROM; hook operations (run_before/run_after/override/disable) | deferred |
| [s08-hook-ordering.md](core/s08-hook-ordering.md) | §8 Hook Ordering | Onion model; discovery order governs hook precedence | deferred |
| [s09-calling-pipelines.md](core/s09-calling-pipelines.md) | §9 Calling Pipelines as Steps | Sub-pipeline isolation; failure propagation | deferred |
| [s10-named-pipelines.md](core/s10-named-pipelines.md) | §10 Named Pipelines | Multiple named pipelines in one file — syntax reserved, not yet impl | deferred |
| [s11-template-variables.md](core/s11-template-variables.md) | §11 Template Variables | `{{ }}` syntax; all variable paths incl. `{{ step.<id>.result }}` for context steps | partial — core variables impl; `{{ step.<id>.result }}` specced, not yet impl |
| [s12-conditions.md](core/s12-conditions.md) | §12 Conditions | `condition:` field; named conditions (if_code_changed, etc.) | deferred |
| [s13-hitl-gates.md](core/s13-hitl-gates.md) | §13 HITL Gates | pause_for_human; tool permission flow diagram | deferred |
| [s14-built-in-modules.md](core/s14-built-in-modules.md) | §14 Built-in Modules | ail/janitor, ail/security-audit, ail/test-writer, etc. | deferred |
| [s15-providers.md](core/s15-providers.md) | §15 Providers | Provider strings; aliases; `resume:` for session continuity | deferred |
| [s16-error-handling.md](core/s16-error-handling.md) | §16 Error Handling | on_error: continue / pause_for_human / abort_pipeline / retry | deferred |
| [s17-materialize.md](core/s17-materialize.md) | §17 materialize | CLI command; output format with origin comments | partial — single-file flatten + origin comments ✓; `FROM` chain traversal/`--expand-pipelines` not impl |
| [s18-complete-examples.md](core/s18-complete-examples.md) | §18 Complete Examples | Full worked YAML — simplest, solo dev, org base, multi-speed | needs update for new step types |
| [s19-runners-adapters.md](core/s19-runners-adapters.md) | §19 Runners & Adapters | Three-tier runner model; runner config; contract summary | reference |
| [s20-mvp.md](core/s20-mvp.md) | §20 MVP v0.0.1 Scope | What is and isn't in scope for v0.0.1 | reference — v0.0.1 complete; alpha scope is next |
<<<<<<< HEAD
<<<<<<< HEAD
| [s21-planned-extensions.md](core/s21-planned-extensions.md) | §21 Planned Extensions | Structured I/O, parallel steps, multi-provider quality comparison (D-020), self-modifying pipelines (D-019), MCP, plugins, observability | planned |
| [s22-open-questions.md](core/s22-open-questions.md) | §22 Open Questions | Unresolved design questions (completion detection, hot reload, self-modifying pipeline approval/validation, etc.) | reference |
=======
| [s21-planned-extensions.md](core/s21-planned-extensions.md) | §21 Planned Extensions | Structured I/O, parallel steps, MCP, plugins, observability | planned |
| [s22-open-questions.md](core/s22-open-questions.md) | §22 Open Questions | Unresolved design questions (completion detection, hot reload, etc.) | reference |
>>>>>>> cbba6f6... wip
=======
| [s21-planned-extensions.md](core/s21-planned-extensions.md) | §21 Planned Extensions | Structured I/O, parallel steps, multi-provider quality comparison (D-020), self-modifying pipelines (D-019), MCP, plugins, observability | planned |
| [s22-open-questions.md](core/s22-open-questions.md) | §22 Open Questions | Unresolved design questions (completion detection, hot reload, self-modifying pipeline approval/validation, etc.) | reference |
>>>>>>> 8449d90... wip: planned extensions

---

## Runner Spec — `spec/runner/`

The AIL Runner Contract — for CLI tool authors and adapter writers.

| File | Section | One-line summary | Status |
|---|---|---|---|
| [r01-overview.md](runner/r01-overview.md) | Purpose, Background, Compliance Tiers | What `ail` needs from any runner; minimum vs. extended compliance | reference |
| [r02-claude-cli.md](runner/r02-claude-cli.md) | Reference Implementation — Claude CLI | Verified flags, event stream, session continuity, tool permission interface | partial — invocation/session-continuity/`--allowedTools` ✓; `--permission-prompt-tool` HITL interface not impl |
| [r03-targets.md](runner/r03-targets.md) | Known Runners, Custom Adapters, Open Questions | Roadmap runners; Runner trait for adapters; remaining open questions | planned |
