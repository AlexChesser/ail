# LEARNINGS

---

## Phase 1 — Workspace Skeleton

### Discoveries not covered by the reference documents
- `cargo fmt` enforces single-line method chains for short builder patterns (e.g. `tracing_subscriber::fmt().json().init()`). Not a concern, but worth noting for future tracing setup in later phases.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- `ail/src/main.rs` currently prints `ail {version}` to stdout in addition to the tracing JSON event on startup. This is intentional for Phase 1 verification. Phase 3 will restructure `main()` to route all output through the proper CLI command handlers.
- The `Cli` struct is currently empty. Phase 3 adds the full argument surface.

### Flags for human review
- None.

---

## Phase 3 — CLI Argument Surface

### Discoveries not covered by the reference documents
- `cargo fmt` reformats `matches!()` macros and long `try_parse_from` chains. No semantic impact.
- clap derive automatically generates `--version` from `#[command(version = ...)]`; the build prompt's "print `ail 0.0.1`, exit 0" behaviour for `--version` is satisfied by clap natively.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- All unimplemented commands write to `stderr` via `eprintln!`. This is intentional for stubs — Phase 4 and later replace these stubs with real implementations.
- `main.rs` is deliberately minimal. All argument parsing logic lives in `cli.rs`.

### Flags for human review
- None.

---

## Phase 4 — Pipeline Discovery and Parsing

### Discoveries not covered by the reference documents
- `clippy::result_large_err` fires on `Result<_, AilError>` because `AilError` contains a heap-allocated `String`. Suppressed with `#[allow(clippy::result_large_err)]` at the module level in `validation.rs` and `config/mod.rs`. A future phase could box `AilError` at the call site, but that changes the public API surface and is deferred.
- Domain types need `#[derive(Debug)]` so they can be used in `Result::unwrap_err()` in tests. This is a test ergonomics need, not a domain concern. The spec doesn't preclude deriving `Debug` — only `Deserialize`.
- The `falls_back_to_ail_yaml_in_cwd` discovery test mutates the process's current working directory. Nextest runs each test in isolation which makes this safe, but any future test that also mutates CWD in the same binary will need to be aware.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- The DTO-to-domain boundary is structurally enforced: `dto.rs` derives `Deserialize`, `domain.rs` derives only `Debug`, transformation in `validation.rs`. The `#[allow(clippy::result_large_err)]` file-level attribute lives in `validation.rs` and `mod.rs` only.
- `Pipeline::passthrough()` returns zero steps; the executor (Phase 9) must handle this case (no-op, exit 0).

### Flags for human review
- [UNDOC] `#[allow(clippy::result_large_err)]`

---

## Phase 5 — `materialize-chain` Command

### Discoveries not covered by the reference documents
- `serde_yaml::from_str` returns `Result<T, _>`, not `T`, so unit tests asserting YAML validity must check `result.is_ok()`, not call `.is_ok()` on the value itself. Trivial but caught a compile error.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- `materialize()` hardcodes `version: "0.0.1"` in output. Phase 5 scope is single-file only; inheritance chain traversal is deferred. When inheritance is implemented, the `version` emitted should come from the resolved pipeline's version field.
- Prompt strings are serialized with double-quote scalar (`"..."`) and interior quotes/backslashes escaped. This is correct for YAML but may not preserve exact formatting of complex multi-line prompts. For v0.0.1 single-line prompts this is sufficient.

### Flags for human review
- [SPEC] §18 describes output as

---

## Phase 6 — TurnLog and Session

### Discoveries not covered by the reference documents
- Rust's module system disallows naming a submodule the same as its parent module (`session/session.rs` inside `session/`). Renamed to `session/state.rs`. This is a Rust idiom, not a domain decision.
- `clippy::result_large_err` did not fire for `TurnLog` — NDJSON write failures are only `std::io::Error`, not `AilError`.
- Tests that call `TurnLog::append()` must change the working directory to a temp directory, since the NDJSON file is written relative to CWD (`.ail/runs/`). This makes these tests slightly sensitive to CWD mutation. Nextest isolation makes this safe for now.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- `TurnLog::append()` never panics on NDJSON write failure — it logs a warning and continues. The spec (§4.4) says the log is written before the next step runs, but a write failure in a v0.0.1 proof of concept shouldn't abort the pipeline. This is a pragmatic choice flagged for review.
- `TurnEntry.timestamp` is `#[serde(skip)]` — SystemTime does not implement Serialize without additional crate support. A future phase can add chrono/time and serialize it.

### Flags for human review
- [SPEC] §4.4 requires the log to be persisted

---

## Phase 7 — Template Variable Resolution

### Discoveries not covered by the reference documents
- `#[allow(clippy::result_large_err)]` is now required in `template.rs` for the same reason as `validation.rs` and `config/mod.rs` — `AilError` contains `String`.
- `set_var`/`remove_var` in tests are technically not thread-safe on all platforms. Nextest runs tests in isolation which avoids this, but it's worth noting.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- `{{ last_response }}` errors if no turn entries exist. The executor (Phase 9) must only use `last_response` in step 2+ of a pipeline (after the invocation response exists). A pipeline that uses `{{ last_response }}` in its first step will fail at resolve time. This is correct behaviour per SPEC §11 ("silent empty is never permitted").
- The template engine is a simple linear scan — no AST, no nesting. Sufficient for v0.0.1.

### Flags for human review
- None. before the next step runs. The current implementation attempts persistence but continues on failure (with a warning). A strict implementation would abort the step on write failure. "annotated YAML with origin comments". The format `# origin: [N] path` is not prescribed by SPEC — it's an implementation choice. If SPEC later formalises the comment format, this will need updating. is a technical debt marker. A future decision: box `AilError` in return types, or restructure `detail` as `Box<str>`. Not blocking for v0.0.1.

---

## Phase 8 — Runner Trait and Claude CLI Adapter (pre-spike assumptions)

**Written before any code. These are assumptions being tested:**

1. `claude --output-format stream-json -p "<prompt>"` emits NDJSON on stdout and exits 0 on success.
2. The stream includes a `{"type":"result","subtype":"success","result":"<text>","total_cost_usd":<float>,"session_id":"<id>"}` event as the final event.
3. Lines before the `result` event include `{"type":"assistant",...}` and `{"type":"user",...}` tool interaction events, and possibly `{"type":"system","subtype":"init",...}`.
4. The process exits after the `result` event — we don't need to close stdin or send any signal.
5. `--output-format stream-json` works with `-p` (non-interactive) with no PTY required.
6. `total_cost_usd` is always present in the result event for successful runs.
7. Stderr from the claude process carries error messages on failure.

**Post-spike findings will be updated below.**

### Discoveries not covered by the reference documents
- `--output-format stream-json` **requires `--verbose`** when used with `-p`. Without `--verbose`, claude exits with: "When using --print, --output-format=stream-json requires --verbose". RUNNER-SPEC.md does not document this. The correct invocation is `claude --output-format stream-json --verbose -p "<prompt>"`.
- The claude CLI blocks nested sessions via the `CLAUDECODE` env var. `ClaudeCliRunner` must remove `CLAUDECODE` from the child process environment using `.env_remove("CLAUDECODE")` on the `Command` builder.
- When run from inside a Claude Code bash tool (even with `env -u CLAUDECODE`), the command appeared to hang. The user confirmed it works when run manually outside the session. This means integration tests for `ClaudeCliRunner` must be `#[ignore]` — they cannot be run from within a Claude Code session.

### Assumptions that proved wrong
- **Assumption #1 was partially wrong**: `claude --output-format stream-json -p "<prompt>"` alone is insufficient — `--verbose` is also required.
- All other assumptions confirmed: three-event stream (system/init, assistant, result), `result` event carries `result` text and `total_cost_usd`, process exits cleanly after `result`.

### Decisions made that future phases should know about
- The `runner/` module exports a `Runner` trait, `StubRunner` (for unit tests), and `ClaudeCliRunner` (real implementation).
- `ClaudeCliRunner` does not capture stderr — on error, `claude` sets `result.subtype = "error"` and `result.is_error = true` in the NDJSON stream. Stderr is not needed for error detection.
- The `result` event's `result` field (not `message.content`) is the canonical response text. The `assistant` event content is streaming/partial.

### Flags for human review
- [UNDOC] `--verbose` is required alongside `--output-format stream-json --print`. This is undocumented in RUNNER-SPEC.md. RUNNER-SPEC.md should be updated.
- [ARCH] Integration tests for `ClaudeCliRunner` cannot run inside a Claude Code session. This means CI will need to either run them outside, or skip them. Annotated `#[ignore]` for now.

---

## Phase 2 — Error Type Foundation

### Discoveries not covered by the reference documents
- `AilError` needs a `Debug` impl to satisfy trait bounds in future phases (e.g. `Result<T, AilError>` used in test assertions). Implemented as a delegating `Display` call — no extra information.

### Assumptions that proved wrong
- None.

### Decisions made that future phases should know about
- `AilError` does not implement `From<std::io::Error>` or other conversions yet. Those will be added in the phases that produce those error types (Phase 4 file I/O, Phase 8 runner I/O).
- `error_types` constants are `pub mod` within `error.rs`, re-exported via `pub mod error` in `lib.rs`. Callers use `ail_core::error::error_types::*`.

### Flags for human review
- None.
