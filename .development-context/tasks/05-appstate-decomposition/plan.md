# Task 05: AppState Decomposition ✓ DONE

## Findings Addressed
- **SRP-001** (high): AppState is an 873-line god object with 38 fields and 7+ responsibilities

## Can Be Done Independently

No dependencies on other tasks.

## Problem Summary

`AppState` in `ail/src/tui/app.rs` holds 38 fields across 7+ distinct responsibilities, violating ARCHITECTURE.md §2.6 (SRP): "Each module, struct, and function has one reason to change."

## Proposed Sub-Structs

| Sub-Struct | Fields | Methods |
|---|---|---|
| **PromptState** | input_buffer, cursor_pos, prompt_preferred_col, prompt_history, history_index, pending_prompt | input_insert, input_backspace, cursor_*, submit_input, history_up/down |
| **PickerState** | open, entries, filter, filtered, cursor, pending_pipeline_switch | open_picker, close_picker, type_char, backspace, nav_up/down, select |
| **PermissionState** | tx, request, modal_open, cursor, session_allowlist | handle_permission_request, nav_up/down, approve_once/session, deny |
| **InterruptState** | pause_flag, kill_flag, modal_open, pending_injection | request_resume, request_kill |
| **ViewportState** | lines, scroll, active_step_id, step_streamed, step_outputs, step_order, viewing_step | append_text, echo_prompt, session_prev/next |
| **RunStats** | run_start, run_end, cumulative_cost_usd, cumulative_input/output_tokens, current_step_index, total_steps, last_session_id | reset |
| **SidebarState** | cursor, disabled_steps | — |

### Resulting AppState

```rust
pub struct AppState {
    pub running: bool,
    pub pipeline: Option<Pipeline>,
    pub steps: Vec<StepDisplay>,
    pub phase: ExecutionPhase,
    pub prompt: PromptState,
    pub viewport: ViewportState,
    pub picker: PickerState,
    pub permissions: PermissionState,
    pub interrupt: InterruptState,
    pub stats: RunStats,
    pub sidebar: SidebarState,
}
```

## Implementation Phases

### Phase 1: Extract data-only sub-structs (mechanical, no behavior change)

Define sub-structs in `app.rs`. Move fields. Update `AppState::new()`. Update all field access across the codebase. The compiler will find every site.

### Phase 2: Move methods to sub-structs

Move prompt methods to `impl PromptState`, picker methods to `impl PickerState`, etc.

For cross-cutting methods (permission approve/deny that log to viewport, interrupt resume that changes phase), keep thin wrapper methods on `AppState`:

```rust
impl AppState {
    pub fn perm_approve_once(&mut self) {
        let tool = self.permissions.request.as_ref()
            .map(|r| r.tool_name.as_str()).unwrap_or("?");
        self.viewport.append_text(&format!("\n  [permission: {tool} — approved once]"));
        self.permissions.approve_once();
    }
}
```

### Phase 3 (optional): Extract into separate modules

Move each sub-struct to `tui/state/<name>.rs`. Defer until file exceeds ~500 lines again.

## Detailed Field Access Map

### `input.rs` changes
| Old | New |
|---|---|
| `app.perm_modal_open` | `app.permissions.modal_open` |
| `app.interrupt_modal_open` | `app.interrupt.modal_open` |
| `app.picker_open` | `app.picker.open` |
| `app.input_buffer` | `app.prompt.input_buffer` |
| `app.cursor_pos` | `app.prompt.cursor_pos` |
| `app.pending_prompt` | `app.prompt.pending_prompt` |
| `app.input_insert(c)` | `app.prompt.input_insert(c)` |
| `app.picker_nav_up()` | `app.picker.nav_up()` |
| `app.session_prev()` | `app.viewport.session_prev()` |

### `inline/mod.rs` changes
| Old | New |
|---|---|
| `app.viewport_lines` | `app.viewport.lines` |
| `app.pause_flag` | `app.interrupt.pause_flag` |
| `app.kill_flag` | `app.interrupt.kill_flag` |
| `app.perm_tx` | `app.permissions.tx` |
| `app.pending_prompt` | `app.prompt.pending_prompt` |
| `app.disabled_steps` | `app.sidebar.disabled_steps` |
| `app.pending_pipeline_switch` | `app.picker.pending_pipeline_switch` |

### `ui/statusbar.rs` changes
| Old | New |
|---|---|
| `app.total_steps` | `app.stats.total_steps` |
| `app.active_step_id` | `app.viewport.active_step_id` |
| `app.cumulative_input_tokens` | `app.stats.cumulative_input_tokens` |
| `app.run_start` | `app.stats.run_start` |

### `ui/modal.rs`, `ui/prompt.rs`, `ui/picker.rs` — similar prefix changes

## Borrow Checker Considerations

- `apply_executor_event` stays on `AppState` as dispatcher — Rust allows borrowing disjoint fields.
- `reset_for_run` stays on `AppState`, delegates to `self.stats.reset()`, `self.viewport.reset()`, etc.
- Cross-cutting methods (perm approve/deny + viewport logging) stay as AppState wrappers.

## Testing

- `cargo build` after Phase 1 catches all compile errors
- No behavioral changes — pure refactor
- Enables future unit testing of sub-structs in isolation (task 06)

## Critical Files
- `ail/src/tui/app.rs` — define sub-structs, update AppState, move methods
- `ail/src/tui/input.rs` — update all field access and method calls
- `ail/src/tui/inline/mod.rs` — update field access
- `ail/src/tui/ui/prompt.rs` — update field access
- `ail/src/tui/ui/modal.rs` — update field access
- `ail/src/tui/ui/picker.rs` — update field access
- `ail/src/tui/ui/statusbar.rs` — update field access
