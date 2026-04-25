# Spec Drift Audit & Implementation Roadmap

**Date:** April 2026  
**Branch:** claude/spec-drift-audit-ClaIG  
**Status:** Complete

## Executive Summary

AIL's specification (24 core sections + 4 runner sections = 28 total) is the primary published artifact. The implementation is meant to prove the spec correct. However, over time, the codebase has advanced beyond some spec sections and left others behind. This comprehensive audit identifies all gaps between specification promises and actual implementation.

### Top 5 Critical Gaps

1. **s09 "deferred" but sub-pipelines fully implemented** — Spec says "deferred" but `execute_sub_pipeline()` in executor.rs works, has depth guards, template variable resolution, and passes tests. Need to update spec/README.md status.

2. **s06 Skills spec exists, implementation is stub** — SKILL.md format, `$ARGUMENTS` substitution, and REPL discovery are specified. Code has `StepBody::Skill(PathBuf)` but `execute_inner()` aborts with `PIPELINE_ABORTED`. Test s15_skills.rs is `#[ignore]`.

3. **s05 `append_system_prompt`/`before`/`then` — spec promises, code doesn't parse** — DTO has no fields for these. These are spec-promised step fields with no implementation path.

4. **s11 template variables — spec says not impl, but it is** — Spec says "`step.<id>.result` not yet impl" but `template.rs` resolves `.result`, `.stdout`, `.stderr`, `.exit_code` from context steps. Tests pass.

5. **`ail logs` command has no spec section** — Full SQLite-backed session listing with FTS search is implemented (`ail/src/logs.rs`, `ail-core/src/logs.rs`). Only `ail log` (singular) is specced; `ail logs` (plural) is completely unspecced.

---

## Spec Coverage Matrix

**Legend:**  
- ✓ = Implemented fully  
- ⚠ = Partial/stub implementation  
- ✗ = Not implemented  
- — = Not applicable (design doc, reference, roadmap)

### Core Spec (s01–s24)

| # | Section | Title | Spec Status | Impl Status | Gap |
|---|---------|-------|-------------|-------------|-----|
| 01 | Purpose & Philosophy | Design grounding | alpha | — | None |
| 02 | Vocabulary | Term definitions | reference | — | **Minor** — needs `context:` step type |
| 03 | File Format | Discovery & YAML schema | partial | ✓ core | **Major** — `FROM`, `meta`, `providers`, `defaults.timeout_seconds`, `defaults.tools` not parsed |
| 04 | Execution Model | Step execution, core invariant | partial | ✓ core | **Major** — s04.3 hooks/conditions not impl |
| 05 | Step Specification | Four step types, on_result | alpha | ⚠ partial | **Major** — `append_system_prompt`, `before`, `then` not parsed; `skill:` returns error |
| 06 | Skills | SKILL.md format, $ARGUMENTS | alpha | ✗ stub | **Critical** — major feature gap |
| 07 | Pipeline Inheritance | FROM, hook operations | deferred | ✗ | None (explicitly deferred) |
| 08 | Hook Ordering | Onion model, precedence | deferred | ✗ | None (explicitly deferred) |
| 09 | Calling Pipelines | Sub-pipeline isolation | deferred (spec) | ✓ works | **Critical** — spec says deferred, but fully implemented |
| 10 | Named Pipelines | Multiple named pipelines | deferred | ✗ | None (explicitly deferred) |
| 11 | Template Variables | `{{ }}` syntax, all paths | partial | ✓ works | **Critical** — spec says `step.<id>.result` not impl, but it works |
| 12 | Conditions | `condition:` field | deferred | ✗ | None (explicitly deferred) |
| 13 | HITL Gates | `pause_for_human`, tool flow | deferred | ⚠ partial | **Major** — works in controlled executor, not in --once text |
| 14 | Built-in Modules | ail/janitor, ail/security-audit | deferred | ✗ | None (explicitly deferred) |
| 15 | Providers | Provider strings, aliases, resume | partial | ⚠ partial | **Major** — `model@provider` syntax, aliases, `resume:` not impl |
| 16 | Error Handling | on_error: actions, retry | deferred | ✗ | None (explicitly deferred) |
| 17 | Materialize | Output, origin comments | partial | ✓ core | **Major** — `FROM` chain traversal, `--expand-pipelines` not impl |
| 18 | Examples | Worked YAML examples | needs update | — | **Major** — examples don't reflect new step types |
| 19 | Runners & Adapters | Runner trait, three-tier model | reference | ✓ | **Minor** — RunnerFactory + per-step `runner:` field work but not highlighted |
| 20 | MVP v0.0.1 | Scope definition | reference | ✓ complete | None |
| 21 | Planned Extensions | Roadmap features | planned | — | None |
| 22 | Open Questions | Design decisions TBD | reference | — | None |
| 23 | Structured Output | NDJSON events, schema | v0.1 | ✓ | None — implementation correct |
| 24 | Log Command | `ail log` CLI | alpha | ✓ + extra | **Major** — spec covers `ail log`, not `ail logs` (plural) |

### Runner Spec (r01–r04)

| # | Section | Title | Impl Status | Gap |
|---|---------|-------|-------------|-----|
| 01 | Overview | Runner contract minimum | ✓ | None |
| 02 | Claude CLI | Reference impl, flags, events | ✓ | **Minor** — streaming, permission bridge, MCP bridge all work but details not in spec |
| 03 | Targets | Roadmap runners | ⚠ | None (roadmap) |
| 04 | AIL Log Format | ail-log/1 markdown format | ✓ | None — implementation correct |

---

## Gap Catalog

### Category A: Spec Promises Not Implemented

These are features explicitly promised in the spec text with no corresponding implementation (either stubbed or missing entirely).

| Feature | Spec Section | Effort | Notes | Files |
|---------|--------------|--------|-------|-------|
| `skill:` step execution | s06 | **L** | Full SKILL.md parser, $ARGUMENTS substitution, REPL discovery | executor.rs line 384–396 (stub), StepBody::Skill |
| Pipeline inheritance (`FROM`) | s07 | **L** | Hook operations, merge semantics, discovery order | None |
| Hook ordering (onion model) | s08 | **M** | Depends on s07 | None |
| Named pipelines | s10 | **S** | Syntax reserved but not parsed | None |
| `condition:` field | s12 | **M** | if_code_changed, if_response_changed, etc. | None |
| `on_error:` handling | s16 | **M** | retry, continue, pause_for_human, abort_pipeline actions | None |
| Built-in modules | s14 | **L** | ail/janitor, ail/security-audit, ail/test-writer | None |
| `append_system_prompt:` field | s05 | **S** | Parse + pass to runner | dto.rs (missing field) |
| `before:`/`then:` step hooks | s05 | **M** | Pre/post execution hooks per step | None |
| `defaults.timeout_seconds` | s03 | **S** | Parse + enforce in executor | dto.rs (missing field) |
| `defaults.tools` | s03 | **S** | Parse + merge with per-step tools | dto.rs (missing field) |
| `FROM`/`meta`/`providers` top-level fields | s03 | **M** | Top-level YAML fields not parsed | None |
| `--expand-pipelines` for materialize | s17 | **M** | Traverse FROM chain, expand all nested pipelines | materialize.rs |
| Provider string format/aliases | s15 | **S** | `model@provider` syntax, e.g. `claude@anthropic` | None |
| `resume:` for session continuity | s15 | **M** | Provider-level session resume, model continuity | None |

### Category B: Code Implements but Spec Missing or Wrong

These are fully working features in the codebase that either have no spec section or have a spec section that incorrectly describes their status.

| Feature | Code Location | Spec Impact | Notes |
|---------|--------------|-------------|-------|
| Sub-pipeline execution | executor.rs:124–209 | s09 status wrong | Spec says "deferred"; code fully works with depth guards, template resolution, error handling. Need to update spec/README.md status from "deferred" to "implemented". |
| `step.<id>.result/stdout/stderr/exit_code` template vars | template.rs | s11 status wrong | Spec README says "specced, not yet impl"; code fully resolves these from context step TurnEntries. Tests pass (s11_template_variables.rs). Update spec status. |
| `ail logs` command | ail/src/logs.rs, ail-core/src/logs.rs | No spec | SQLite-backed session listing with FTS search, prefix filtering, `--tail`, JSON output. Only `ail log` is specced; `ail logs` (plural) is completely unspecced. |
| `ail stdio` command | ail/src/stdio.rs | No spec | Multi-turn machine-facing NDJSON protocol over stdin/stdout. No spec section at all. |
| TUI mode | ail/src/tui/ | No spec | Full terminal UI for interactive pipeline execution. Partial implementation. No spec section. |
| MCP permission bridge | ail/src/mcp_bridge.rs, ail-core/src/ipc.rs | r02 incomplete | Handles tool permission HITL via local IPC. Mentioned in r02 but protocol details not documented. |
| `--model` CLI flag | cli.rs line 33–36 | Not in spec | Override model for all invocations. |
| `--provider-url` CLI flag | cli.rs line 39–41 | Not in spec | Override provider base URL (ANTHROPIC_BASE_URL). |
| `--provider-token` CLI flag | cli.rs line 43–46 | Not in spec | Override provider auth token (ANTHROPIC_AUTH_TOKEN). |
| `--show-thinking` CLI flag | cli.rs line 52–54 | Not in spec | Include model thinking text in --once output. |
| `--show-responses` CLI flag | cli.rs line 56–58 | Not in spec | Include full step response text in --once output. |
| `execute_with_control()` (controlled executor) | executor.rs | s23 partial | Event streaming, pause/kill control, HITL channel. Used by TUI/JSON modes. Not fully documented in spec. |
| `pause_for_human` in controlled executor | executor.rs:169–228 | s13 status wrong | s13 says "deferred"; code has working implementation in `execute_with_control()`. No-op in simple `execute()` (--once text mode). Partial but functional. |
| RunnerFactory + per-step `runner:` field | runner/factory.rs, domain.rs | s19 incomplete | Per-step runner override works. Factory builds runners by name. s19 doesn't describe this. |
| NDJSON stdin control protocol | main.rs:312–366 | Not in spec | `hitl_response`, `permission_response`, `pause`, `resume`, `kill` message types. JSON protocol between extension/consumer and ail. |
| Cross-platform IPC (Windows named pipes) | ipc.rs | Not in spec | Handles Unix domain sockets and Windows named pipes. Not in any spec. |
| SQLite log provider | session/sqlite_provider.rs | No spec | Full-featured SQLite backend for turn log persistence. Tests exist (s13_sqlite_provider.rs) but no spec section documents the provider. |
| ail-log/1 formatter | formatter.rs | r04 | Correctly implements r04 spec. Generates markdown with thinking blocks, tool calls, stdio, cost lines. No gaps here. |
| `on_result: pipeline:` action | executor.rs, domain.rs | s05 incomplete | Conditionally call another pipeline in on_result. Works but not highlighted in s05. |

### Category C: Partial Implementations

These are features where some modes or paths work but others don't. The spec promises uniformity but the implementation is mode-dependent.

| Feature | What Works | What Doesn't | Impact |
|---------|-----------|--------------|--------|
| `pause_for_human` | Works in controlled executor (TUI, JSON `--output-format json`) | No-op in simple `execute()` (--once text mode) | s13 spec doesn't clarify mode dependency |
| Provider config | `defaults.model`, `defaults.provider`, per-step `model:`, CLI `--model` | `model@provider` syntax, provider aliases, `resume:` for session continuity | s15 spec incomplete |
| Materialize | Single-file flatten, origin comment inclusion | No FROM chain traversal, no `--expand-pipelines` flag | s17 incomplete |
| Step fields (s05) | id, prompt, skill(stub), pipeline, action, context, tools, on_result, model, runner, message | `append_system_prompt`, `before`, `then`, `condition` | s05 incomplete, DTO missing fields |

---

## Roadmap: 5 Epics

Organized by priority and impact. Each gap above maps to one of these epics.

### Epic 1: Spec Accuracy Corrections (Priority: **HIGH**, Effort: **Small**)

**Goal:** Fix spec sections that describe the opposite of reality.

**Items:**
- [ ] **s09–Calling Pipelines:** Change status from "deferred" to "implemented". Update spec/README.md line 35.
- [ ] **s11–Template Variables:** Change status from "partial — `step.<id>.result` not yet impl" to "implemented". Update spec/README.md line 37.
- [ ] **s13–HITL Gates:** Update from "deferred" to "partial — implemented in controlled executor". Clarify mode dependency.
- [ ] **s05–Step Specification:** Add documentation for `on_result: pipeline:` action (it works but isn't highlighted).
- [ ] **s02–Vocabulary:** Add definition of `context:` step type (shell commands).

**Verification:** Run tests to confirm functionality. Push spec changes to repo.

---

### Epic 2: Spec Coverage for Existing Code (Priority: **HIGH**, Effort: **Medium**)

**Goal:** Write spec sections for features that exist but aren't documented.

**Items:**
- [ ] **New s24b–`ail logs` Command:** Document the plural `logs` command (session listing, FTS search, filtering). Separate from existing `ail log` (singular, single-run display).
- [ ] **New section–`ail stdio` Command:** Document the bidirectional NDJSON protocol (multi-turn pipeline execution per message, HITL events, permission responses).
- [ ] **New section–CLI Flags:** Document `--model`, `--provider-url`, `--provider-token`, `--show-thinking`, `--show-responses`.
- [ ] **New section–NDJSON Stdin Control Protocol:** Document `hitl_response`, `permission_response`, `pause`, `resume`, `kill` message types for extension/consumer integration.
- [ ] **Expand s19–RunnerFactory:** Document `RunnerFactory::build()`, per-step `runner:` field, case-insensitive matching.
- [ ] **Expand r02–MCP Permission Bridge:** Document local IPC transport (Unix domain sockets, Windows named pipes), protocol messages.
- [ ] **Expand s04–Execution Model:** Document `execute_with_control()`, event streaming, pause/kill signals.

**Verification:** Verify documented features against code. Tests should cover all documented behaviors.

---

### Epic 3: Step Type Completeness (Priority: **MEDIUM**, Effort: **Large**)

**Goal:** Complete the step type system with missing fields and new step kinds.

**Items:**
- [ ] **s06–Skills Execution:** Implement full `skill:` step type. Parse SKILL.md files, substitute `$ARGUMENTS`, handle REPL discovery. Enable s15_skills.rs test. ~2–3 weeks.
- [ ] **s05–`append_system_prompt:` Field:** Parse and implement DTO field, pass to runner as system prompt append. ~2–3 days.
- [ ] **s12–Conditions:** Implement `condition:` field. Support `if_code_changed`, `if_response_contains`, etc. Conditional step skipping. ~1–2 weeks.

**Verification:** All 22 spec test files pass. New tests for each feature.

---

### Epic 4: Pipeline Composition (Priority: **LOW**, Effort: **Large**)

**Goal:** Advanced pipeline composition and reusability.

**Items:**
- [ ] **s07–Pipeline Inheritance:** Implement `FROM` directive, hook operations (run_before, run_after, override, disable). Complex multi-file semantics. ~3–4 weeks.
- [ ] **s08–Hook Ordering:** Implement onion model, discovery-order-driven precedence. Depends on s07. ~1–2 weeks.
- [ ] **s10–Named Pipelines:** Support multiple named pipelines in one YAML file. Syntax already reserved. ~1 week.
- [ ] **s17–Materialize `--expand-pipelines`:** Traverse `FROM` chain, expand all nested pipelines into single file. ~3–5 days.

**Verification:** Integration tests with complex inheritance hierarchies.

---

### Epic 5: Error Handling & Resilience (Priority: **MEDIUM**, Effort: **Medium**)

**Goal:** Complete error handling and step lifecycle features.

**Items:**
- [ ] **s16–`on_error:` Actions:** Implement error handling with retry, continue, pause_for_human, abort_pipeline actions. ~1–2 weeks.
- [ ] **s03–`defaults.timeout_seconds`:** Parse and enforce step execution timeout. ~3–5 days.
- [ ] **s05–`before:`/`then:` Step Hooks:** Pre/post execution hooks per step. ~1 week.
- [ ] **s15–Provider Aliases & `resume:`:** Implement `model@provider` syntax, provider aliases, session continuity. ~1–2 weeks.

**Verification:** Error handling tests cover all action types and modes.

---

## Implementation Notes

### For Future Work: Guidance on Using This Audit

**Why this matters:** AIL's spec is the contract. Implementation drift creates technical debt and confuses users. This audit identifies exactly where drift exists and in what direction (spec→code mismatch vs code→spec mismatch).

**How to use this roadmap:**
1. **Start with Epic 1** (Spec Accuracy). These are documentation fixes only — no code changes. Takes 1–2 days.
2. **Move to Epic 2** (Spec Coverage). Write spec sections for existing features. These inform future implementation and extension. Takes 1–2 weeks.
3. **Epic 3–5 are feature implementation epics.** Each should become separate GitHub issues/milestones.

**Testing strategy:**
- Existing tests (22 spec files in `ail-core/tests/spec/`) provide a comprehensive reference. Each new feature should add tests to its corresponding spec file.
- Run `cargo test` before every commit.
- Run `cargo clippy` and `cargo fmt` to stay clean.

### Key Files to Monitor

**Spec files that need updates:**
- `spec/README.md` (status column)
- `spec/core/s09-calling-pipelines.md` (deferred → implemented)
- `spec/core/s11-template-variables.md` (status fix)
- `spec/core/s13-hitl-gates.md` (deferred → partial)

**Code files that implement unspecced features:**
- `ail/src/logs.rs` (ail logs command)
- `ail/src/stdio.rs` (ail stdio command)
- `ail/src/tui/` (TUI mode)
- `ail-core/src/ipc.rs` (permission bridge)
- `main.rs:312–366` (stdin control protocol)

**Code files that need extension:**
- `ail-core/src/config/dto.rs` (add missing fields: timeout_seconds, tools at defaults level, append_system_prompt, before, then, condition)
- `ail-core/src/executor.rs` (implement skill, condition, on_error handling)
- `ail-core/src/template.rs` (already complete)

---

## Effort Estimates Summary

| Epic | Priority | Total Effort | Dependencies |
|------|----------|--------------|--------------|
| 1. Spec Accuracy | HIGH | **Small** (1–2 days) | None |
| 2. Spec Coverage | HIGH | **Medium** (1–2 weeks) | Epic 1 |
| 3. Step Types | MEDIUM | **Large** (6–8 weeks) | None |
| 4. Pipeline Composition | LOW | **Large** (6–8 weeks) | None |
| 5. Error Handling | MEDIUM | **Medium** (3–4 weeks) | None |

**Total: ~18–22 weeks** for all epics to reach 100% alignment between spec and code.

---

## References

- **Spec:** `spec/` directory (24 core sections + 4 runner sections)
- **Implementation:** `ail-core/src/` (library) + `ail/src/` (CLI binary)
- **Tests:** `ail-core/tests/spec/` (22 integration test files, one per spec section)
- **Architecture:** `ARCHITECTURE.md` (design rationale, roadmap, known constraints)
- **Changelog:** `CHANGELOG.md` (v0.1 features, open questions)
- **Module Docs:** `ail-core/CLAUDE.md` (22 modules, domain types, invariants)

