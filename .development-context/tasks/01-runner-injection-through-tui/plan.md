# Task 01: Runner Injection Through TUI Call Stack ✓ DONE

## Findings Addressed
- **DIP-001** (critical): backend.rs imports and constructs ClaudeCliRunner directly
- **COUPLING-001** (critical): Entire TUI stack has no runner parameter
- **DI-001** (high): spawn_backend() accepts pipeline and cli_provider but not the runner

## Problem Summary

The TUI call chain (`main.rs` -> `tui::run()` -> `inline::run()` -> `spawn_backend()`) directly imports and constructs `ClaudeCliRunner` inside `backend.rs`, violating ARCHITECTURE.md §2.7: "Dependencies are injected, not constructed internally." The runner should be constructed at the composition root (`main.rs`) and threaded through the call stack.

## Scope Adjustment

The `cli_provider` parameter cannot be cleanly removed in this task. It is used by `Session` and the executor's per-step provider resolution chain (`executor.rs` lines 224, 503). Removing it requires a deeper refactor. This plan defers that.

## Implementation Steps

### Step 1: Add `needs_permission_socket()` to the Runner trait

**File:** `ail-core/src/runner/mod.rs`

The `headless` flag in backend.rs controls both runner construction (eliminated by injection) and permission socket creation (still needed). Add a default method:

```rust
fn needs_permission_socket(&self) -> bool {
    false
}
```

**File:** `ail-core/src/runner/claude.rs` — override:
```rust
fn needs_permission_socket(&self) -> bool {
    !self.headless
}
```

StubRunner gets the default `false` for free.

### Step 2: Modify `spawn_backend()` signature

**File:** `ail/src/tui/backend.rs`

Change from:
```rust
pub fn spawn_backend(
    pipeline: Option<Pipeline>,
    cli_provider: ProviderConfig,
    headless: bool,
) -> (mpsc::Sender<BackendCommand>, mpsc::Receiver<BackendEvent>)
```

To:
```rust
pub fn spawn_backend(
    pipeline: Option<Pipeline>,
    cli_provider: ProviderConfig,
    runner: Box<dyn Runner + Send>,
) -> (mpsc::Sender<BackendCommand>, mpsc::Receiver<BackendEvent>)
```

Inside the function:
- Remove `use ail_core::runner::claude::ClaudeCliRunner;`
- Remove `let runner = ClaudeCliRunner::new(headless);`
- Replace `if !headless` with `if runner.needs_permission_socket()`
- Use `&*runner` when passing to executor (which takes `&dyn Runner`)

### Step 3: Thread runner through `inline::run()` and `run_app()`

**File:** `ail/src/tui/inline/mod.rs`

Replace `headless: bool` with `runner: Box<dyn ail_core::runner::Runner + Send>` in both `run()` and `run_app()` signatures. Pass through to `spawn_backend()`.

### Step 4: Thread runner through `tui::run()`

**File:** `ail/src/tui/mod.rs`

Replace `headless: bool` with `runner: Box<dyn ail_core::runner::Runner + Send>`. Pass through to `inline::run()`.

### Step 5: Construct runner in `main.rs` (composition root)

**File:** `ail/src/main.rs`

```rust
let runner = Box::new(ail_core::runner::claude::ClaudeCliRunner::new(cli.headless));
if let Err(e) = tui::run(pipeline, cli_provider, runner) {
```

The `--once` branch already constructs the runner at line 112 — no change needed there.

## Implementation Order

1. Step 1 (add trait method — ail-core, no breakage)
2. Steps 2-4 together (bottom-up signature changes)
3. Step 5 (update main.rs call site)

All steps form a single atomic commit.

## Risks and Mitigations

- **`Send` bound**: `ClaudeCliRunner` contains only `String` and `bool` — it is `Send`. `StubRunner` uses `AtomicU32` — also `Send`.
- **Borrow vs ownership**: Backend thread owns `Box<dyn Runner + Send>`, passes `&*runner` to executor.
- **Permission socket regression**: `needs_permission_socket()` preserves the existing behavior without a separate `headless` flag.

## Testing

- All existing tests pass unchanged (StubRunner gets default `false` for free)
- `#[ignore]` integration tests unaffected
- Manual verification: launch TUI with and without `--headless`

## Critical Files
- `ail-core/src/runner/mod.rs`
- `ail-core/src/runner/claude.rs`
- `ail/src/tui/backend.rs`
- `ail/src/tui/inline/mod.rs`
- `ail/src/tui/mod.rs`
- `ail/src/main.rs`
