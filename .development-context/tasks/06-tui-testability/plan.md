# Task 06: TUI Testability

## Findings Addressed
- **QUALITY-001** (medium): AppState and inline event loop are untestable, violating ARCHITECTURE.md §2.10

## Benefits From
- Task 05 (AppState decomposition) — but can be started independently

## Problem Summary

The TUI has no unit tests. Six methods on AppState perform side effects (channel sends, atomic flag writes), making them untestable without real channels. The inline event loop mixes I/O with state transitions.

## Design: Functional Core, Imperative Shell

Separate side effects from state updates using a `SideEffect` enum. Methods return descriptions of effects; the event loop executes them.

## Implementation

### Phase 1: Introduce SideEffect enum

**File:** `ail/src/tui/app.rs`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SideEffect {
    SendPermissionResponse(PermissionResponse),
    SetPauseFlag(bool),
    SetKillFlag,
}
```

Refactor the six side-effectful methods to return `Vec<SideEffect>`:

- `handle_permission_request()` → returns `SendPermissionResponse(Allow)` when auto-allowing, else `vec![]`
- `perm_approve_once()` → returns `SendPermissionResponse(Allow)`
- `perm_approve_session()` → returns `SendPermissionResponse(Allow)`
- `perm_deny()` → returns `SendPermissionResponse(Deny(...))`
- `request_resume()` → returns `SetPauseFlag(false)`
- `request_kill()` → returns `[SetKillFlag, SetPauseFlag(false)]`

### Phase 2: Update event loop to execute effects

**File:** `ail/src/tui/inline/mod.rs`

Add helper:
```rust
fn execute_effects(effects: &[SideEffect], app: &AppState) {
    for effect in effects {
        match effect {
            SideEffect::SendPermissionResponse(resp) => {
                if let Some(ref tx) = app.perm_tx { let _ = tx.send(resp.clone()); }
            }
            SideEffect::SetPauseFlag(val) => {
                if let Some(ref f) = app.pause_flag { f.store(*val, Ordering::SeqCst); }
            }
            SideEffect::SetKillFlag => {
                if let Some(ref f) = app.kill_flag { f.store(true, Ordering::SeqCst); }
            }
        }
    }
}
```

**File:** `ail/src/tui/input.rs`

Change `handle_event` to return `Vec<SideEffect>`. Each branch that calls a side-effectful method forwards the returned effects.

### Phase 3: Extract loop-body logic

**File:** `ail/src/tui/app.rs` (or new `tui/loop_logic.rs`)

Extract pending prompt dispatch:
```rust
pub enum PromptAction {
    SendHitl(String),
    SubmitToBackend { prompt: String, disabled_steps: HashSet<String> },
    None,
}

impl AppState {
    pub fn resolve_pending_prompt(&mut self) -> PromptAction { ... }
}
```

Extract pipeline switch:
```rust
pub fn apply_pipeline_switch(&mut self, new_pipeline: Pipeline, name: String) { ... }
pub fn apply_pipeline_switch_error(&mut self, detail: &str) { ... }
```

### Phase 4: Add tests

**File:** `ail/src/tui/app.rs` — `#[cfg(test)] mod tests`

**Already-pure methods (can test now):**
- `apply_executor_event` — StepStarted/Completed/Failed/Skipped, StreamDelta, PipelineCompleted, HitlGate, CostUpdate
- Prompt input — insert, backspace, cursor navigation, submit, history
- Picker — open, filter, select, backspace-closes
- Session navigation — prev/next

**After SideEffect refactor:**
- Permission — auto-allow returns effect, unknown opens modal, approve/deny returns correct effect
- Interrupt — resume returns SetPauseFlag(false), kill returns both effects

**After loop logic extraction:**
- resolve_pending_prompt — HitlGate vs normal, returns correct action
- apply_pipeline_switch — steps rebuilt, sidebar reset

**Note:** `PermissionResponse` may need `PartialEq` derive in `ail-core/src/runner/mod.rs` for test assertions.

### Phase 5: Update input.rs to thread effects (part of Phase 2)

Must ship atomically with Phase 1-2 since method signatures change.

## Sequencing

1. Tests for already-pure methods (no refactoring needed) — immediate value
2. SideEffect enum + refactor methods + input.rs + event loop (atomic)
3. Tests for newly-pure methods
4. Extract loop-body logic + tests

## Potential Challenges

- **`Instant::now()` in apply_executor_event** — assert `is_some()` not exact value
- **Compile-time breakage** — Phases 1, 2, 5 must be atomic (single commit)
- **perm_tx/flags remain on AppState** — fields exist for event loop to read, but methods no longer touch them

## Critical Files
- `ail/src/tui/app.rs` — SideEffect enum, refactor methods, add tests
- `ail/src/tui/input.rs` — thread `Vec<SideEffect>` through handle_event
- `ail/src/tui/inline/mod.rs` — execute_effects helper, update loop
- `ail-core/src/runner/mod.rs` — may need PartialEq on PermissionResponse
