# AIL Code Critique Report — 2026-04-11

**Subject:** ail-core and ail Rust projects checked against the spec, architecture, and design.

**Contributing agents:** Staff Engineer, Junior, Conformer, Devil's Advocate, Verifier, Simplifier, Stickler, Architect

**Historical context written to:** `/tmp/critique_1775883095833654903_history.md`

---

## Blocking Concerns

### 1. HTTP runner is unsafe for any real use: no timeouts, no cancellation, lying session IDs, unbounded memory

`runner/http.rs:234-242` constructs requests with no read/connect timeouts — a stalled Ollama or remote API hangs the entire pipeline forever. `options.cancel_token` is ignored entirely. The in-process `HashMap<String, Vec<ChatMessage>>` (`http.rs:99`) grows without bound and is not persisted — when the process exits the session is gone, yet `runner_session_id` is written to the turn log as if the session is resumable. A user reading `ail logs` and trying to resume an Ollama session will silently start a brand-new empty conversation. Each invoke also resends the full history, making token cost O(n²) in turns. Finally, `build_step_runner_box` constructs a fresh `HttpRunner` per step (`helpers.rs:64`), meaning even a two-step pipeline sharing `runner: ollama` gets two independent session stores — session continuity is defeated silently.
*(Devil's Advocate, Stickler, Conformer, Architect)*

> **Fix:** Add explicit `ureq::AgentBuilder` timeouts; honor `cancel_token` in a worker thread; extract the session store to a shared `Arc` passed at runner construction; document that the session ID in the log is NOT resumable for the HTTP runner.

---

### 2. Audit trail is not durable and errors are swallowed

`JsonlProvider::write_entry` (`log_provider.rs:72-81`) does `OpenOptions::append → writeln!` with no `flush()` or `fsync()`. The spec (§4.4) calls this "a durable, structured record written to disk before the next step begins" — that claim is false; kernel buffering and power loss can lose the most recent entries. Worse: errors from `write_entry` are `tracing::warn!` and discarded; `CompositeProvider::write_entry` returns `Ok(())` even when *all* underlying providers fail (`log_provider.rs:104-112`). The audit trail can silently stop being written while the pipeline runs to completion.
*(Devil's Advocate, Stickler)*

> **Fix:** Add `file.sync_data()` after each write, or at minimum after each step completes. Return `Err` from `CompositeProvider` if all providers fail.

---

### 3. `on_result: pipeline:` silently corrupts template resolution for the triggering step

`executor/core.rs:479-494`: when `on_result: pipeline:` fires, `execute_sub_pipeline` appends a `TurnEntry` with the *same step_id* as the parent prompt. `TurnLog::response_for_step` uses `iter().find()` — it returns the first match, not the most recent. Downstream `{{ step.<id>.response }}` resolves to the original prompt's response, not the sub-pipeline's. There is no spec entry for this collision.
*(Stickler)*

> **Fix:** Use a derived ID (`<id>__on_result`) for the sub-pipeline entry, or change `response_for_step` to `iter().rev().find()`. Update SPEC §11.

---

### 4. `on_result: pause_for_human` silently continues in headless mode — safety gate is broken

`executor/core.rs:476-478` delegates to `NullObserver::on_result_pause` which emits a single `tracing::info!` and returns. The pipeline continues as if the pause was acknowledged. For users who use `pause_for_human` as a circuit-breaker after a risky step (which is its documented purpose), this is a silent safety regression in headless/`--once` mode.
*(Stickler)*

> **Fix:** `NullObserver::on_result_pause` should either abort with `PIPELINE_ABORTED` or print a visible warning to stderr.

---

### 5. `AilError::PipelineAborted` used as catch-all in `logs.rs` and `delete.rs`

`ail-core/src/logs.rs` (21 sites) and `ail-core/src/delete.rs` (12 sites) use `PipelineAborted` for SQLite open failures, query errors, "run not found," etc. None of these are pipeline aborts. Downstream tooling (VS Code extension, NDJSON consumers) that keys off `error_type()` strings cannot distinguish a genuine abort from a storage error.
*(Conformer)*

> **Fix:** Add `STORAGE_QUERY_FAILED` and `RUN_NOT_FOUND` constants to `error::error_types` and map appropriately.

---

### 6. Per-step runner overrides are always `headless: true`, ignoring the CLI flag

`executor/helpers.rs:64`: `RunnerFactory::build(name, true)` hardcodes headless regardless of how `ail` was invoked. A user running without `--headless` who also writes a step with `runner: claude` gets `--dangerously-skip-permissions` silently injected mid-pipeline.
*(Conformer)*

> **Fix:** Thread the parent runner's headless mode through `Session` or the executor config, and use it in `build_step_runner_box`.

---

### 7. Test file numbers are systematically misaligned with spec section numbers

`ail-core/tests/spec/mod.rs` uses numbering from a prior spec organization:

| Test file | Maps to (actual spec) |
|---|---|
| `s06_pipeline_inheritance.rs` | `s07-pipeline-inheritance.md` |
| `s08_http_runner.rs`, `s08_multi_runner.rs` | `s19-runners-adapters.md`, `spec/runner/r05-http-runner.md` |
| `s09_tool_permissions.rs` | `s09-calling-pipelines.md` |
| `s15_skills.rs` | `s06-skills.md` |
| `s17_error_handling.rs` | `s16-error-handling.md` |
| `s18_materialize.rs` | `s17-materialize.md` |
| `s35_ail_log_formatter.rs`, `s39_consistency.rs`, `s40_delete_run.rs` | No corresponding spec section |

*(Verifier)*

> **Fix:** Rename test files to match current spec section numbers; document the three unnumbered tests under their nearest spec section or add new spec sections.

---

### 8. `{{ step.<id>.tool_calls }}` is documented in spec §11 but not implemented

`spec/core/s11-template-variables.md:17` documents `tool_calls` but `template.rs:71-106` has no match arm for it — it falls through to "not a recognised template variable." A user who writes a pipeline using this documented variable gets a confusing abort.
*(Verifier, Conformer)*

> **Fix:** Either implement `tool_calls` in `template.rs` or strike it from `spec/core/s11` and add a "planned" note.

---

### 9. CLAUDE.md's `--once Flow` step 3 is factually wrong

CLAUDE.md documents: "each step resumes via `--resume <last_runner_session_id>`." The actual code (`executor/core.rs:337-344`) only sets `resume_session_id` when `step.resume == true`. Steps do not resume by default. This inverts the actual default behavior.
*(Stickler)*

> **Fix:** Update CLAUDE.md `--once Flow` step 3 to say "steps run in isolation by default; set `resume: true` on a step to resume the prior session."

---

### 10. `--resume` silently loses context for non-default providers on Claude CLI

`runner/claude/mod.rs:168-174`: the Claude runner only emits `--resume` when no `base_url` extension is present. A pipeline running against Bedrock or Vertex via Claude CLI gets fresh sessions on every step with no signal to the user. Given the HTTP runner was added specifically for full system-prompt control, this is a configuration that users will plausibly reach.
*(Devil's Advocate)*

> **Fix:** Log a visible warning when `step.resume == true` but resume is not being honored due to provider configuration.

---

## Worth Considering

- **Strategy tension: two unmeasured value propositions.** ARCHITECTURE.md argues Rust from "$100k/year at 10k concurrent sessions" (no benchmark exists; the binary exits after `--once`). `spec/core/s01` argues from LLM "dysexecutive syndrome" (unfalsifiable). Neither is measured. The implementation is a for-loop over YAML steps; the spec frames it as a supervisory neocortex. Pick one and measure it before v1. *(Staff Engineer, Junior)*

- **Trajectory incoherence: docs say infra control plane, code is becoming interactive dev tool.** TUI removed → good. Then: REPL added, permission intercepts enriched, AskUserQuestion added. These are valuable but the two framings cannot both be right. *(Staff Engineer)*

- **`ail-core/src/lib.rs` has no curated public API surface.** Every `pub mod` declaration exposes internals to future SDK consumers. Add a `pub mod prelude` with re-exports of the stable surface; demote implementation modules to `pub(crate)` where the binary doesn't need them externally. *(Architect)*

- **`ipc.rs` and `protocol.rs`/`control_bridge.rs` are poorly documented cross-boundary subsystems.** `ipc.rs` re-exports an `interprocess` type as part of ail-core's public API. `protocol.rs` (ail-core) is only consumed by `control_bridge.rs` (binary). Document both as the "ail control-channel protocol" or collapse `protocol.rs` into the binary. *(Architect)*

- **`{{ session.tool }}` hardcoded to `"claude"`** (`template.rs:59`). With the HTTP runner first-class, this returns the wrong value for any Ollama/HTTP step. Thread the active runner name through the session. *(Verifier, Conformer)*

- **`skill:` steps abort at runtime, not at validate-time.** `ail validate` accepts `skill:` steps successfully; the abort only surfaces on execution. Add a `ConfigValidationFailed` error for unimplemented step types at validation. *(Devil's Advocate)*

- **`spec/core/s11` contradicts itself on aliases.** Line 23 says "There are no convenience aliases" but `template.rs:43` and CLAUDE.md both acknowledge `session.invocation_prompt` as a deprecated alias. Fix the spec line. *(Verifier, Conformer)*

- **`spec/core/s19-runners-adapters.md` is significantly stale.** References TUI (removed), dynamic-loading adapter syntax (unimplemented), hardcoded runner names misrepresented as configurable. *(Architect)*

- **`ProviderConfig` cost fields may be dead code.** `input_cost_per_1k` / `output_cost_per_1k` participate in the full merge chain but cost is reported via `RunResult.cost_usd`. Grep and delete if confirmed unused (`config/domain.rs:30-33`, `helpers.rs:resolve_step_provider`). *(Simplifier)*

- **Deprecated `session.invocation_prompt` is still emitted by `Pipeline::passthrough()`** — teaches the deprecated form to anyone reading generated output. *(Simplifier)*

- **`on_result: Contains:` matching is case-insensitive but undocumented** (`executor/helpers.rs:111-114`). Surprises users expecting exact-case. Document in SPEC §5.3. *(Stickler)*

- **CLAUDE.md's `#[allow(clippy::result_large_err)]` list is stale.** Lists 5 files including `executor.rs` (which no longer exists); the attribute now appears in 18+ files post-split. *(Verifier)*

- **`spec/core/s03-file-format.md` has a duplicate `timeout_seconds` block** at lines 71–73. Merge artifact. *(Verifier)*

- **The tagline "deterministic chain" is a category error.** Step *order* is deterministic; step *content* (LLM responses) is not. Users will file bug reports about non-deterministic runs. Suggest "ordered" or "scripted." *(Devil's Advocate)*

- **`executor/headless.rs` is effectively a test file with a one-liner.** The 7-line production wrapper is swamped by 440 lines of tests. Consider moving tests next to `core.rs` or noting this is intentional. *(Simplifier)*

- **`ail/src/ask_user_types/canonical.rs` and `flat.rs` duplicate `parse_options`/`parse_option`** verbatim. Pull into `mod.rs`. *(Simplifier)*

---

## Strengths

1. **The DTO→Domain boundary is genuine and consistently maintained.** `config/domain.rs` has zero serde derives; every new config field goes through the dto→validation→domain pipeline. This is a structural guarantee that has held across 88 commits and pays off when the config schema grows.

2. **The April 5 DI fix is real and complete.** The executor imports only `&dyn Runner`; `ClaudeInvokeExtensions` is now constructed via `Runner::build_extensions()`. The seam is clean: adding a new runner requires exactly three files (implement the trait, add a module, add a factory arm). This is the project's best architectural decision.

3. **The spec-discipline culture works.** The April 4 spec audit caught 5 concrete divergences and produced verified corrections. The rule that functional changes must update spec files is paying off — it is the mechanism that makes the above two strengths visible and enforceable.
