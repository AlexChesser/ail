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
- [SPEC] §18 describes output as "annotated YAML with origin comments". The format `# origin: [N] path` is not prescribed by SPEC — it's an implementation choice. If SPEC later formalises the comment format, this will need updating. is a technical debt marker. A future decision: box `AilError` in return types, or restructure `detail` as `Box<str>`. Not blocking for v0.0.1.

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
