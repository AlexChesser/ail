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
