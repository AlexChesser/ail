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
