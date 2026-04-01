# Issue #1: TUI Pipeline Switching

## Status: ALREADY IMPLEMENTED — NO WORK NEEDED

After full codebase exploration, this feature is completely implemented and functional.

---

## What Exists

### Picker State Machine — `ail/src/tui/app.rs:283-372`

`PickerState` struct with fields:
- `open: bool` — modal visibility
- `entries: Vec<PipelineEntry>` — full pipeline list from `discover_all()`
- `filter: String` — user-typed filter characters
- `filtered: Vec<usize>` — matching entry indices
- `cursor: usize` — current selection
- `pending_pipeline_switch: Option<PathBuf>` — consumed by main loop

Methods: `open_picker()`, `type_char(c)`, `backspace()`, `nav_up()`, `nav_down()`, `select()`, `update_filter()`

### Input Trigger — `ail/src/tui/input.rs:172-185`
- `:` typed in empty buffer while Idle/Completed/Failed → `app.picker.open_picker()`
- All input dispatched to `handle_picker()` while picker is open
- Keys: Enter, Up/Down arrows, `j`/`k`, Backspace, typed chars

### Main Loop — `ail/src/tui/inline/mod.rs:102, 188-202`
- Populates `app.picker.entries = ail_core::config::discovery::discover_all()` at startup
- Checks `pending_pipeline_switch` each tick; calls `ail_core::config::load()` and `app.apply_pipeline_switch()`

### Rendering — `ail/src/tui/ui/picker.rs:13-80`
- Modal above prompt, left-aligned
- Up to 8 visible entries with bullet (•) on selection
- Title shows ` Pipelines ` or ` :<filter> `
- `(no matches)` in dim gray when filter has no results

### Prompt — `ail/src/tui/ui/prompt.rs:25-31`
- Shows `: <filter>` with cursor when picker is open

### Pipeline Discovery — `ail-core/src/config/discovery.rs:54-98`
- `discover_all()` scans in SPEC §3.1 order:
  1. `.ail.yaml` in CWD
  2. `*.yaml`/`*.yml` in `.ail/` in CWD
  3. `*.yaml`/`*.yml` in `~/.config/ail/`
- Returns alphabetically sorted `Vec<PipelineEntry { name, path }>`

### Pipeline Switch — `ail/src/tui/app.rs:862-878`
- `apply_pipeline_switch(pipeline, name)` — rebuilds sidebar, resets state, appends status line
- `apply_pipeline_switch_error(detail)` — appends error to viewport on load failure

### Unit Tests — `ail/src/tui/app.rs:1254-1314`
- `open_picker_populates_filtered_with_all_entries`
- `picker_type_char_filters_entries`
- `picker_backspace_on_empty_filter_closes_picker`
- `picker_select_sets_pending_switch_and_closes`

---

## Verification

```bash
mkdir -p .ail && touch .ail/{alpha,beta,gamma}.yaml
cargo run -- --tui
# Type : in empty prompt → picker appears
# Type 'a' → filtered to 'alpha'
# Backspace → full list restored
# Arrow keys / j/k to navigate
# Enter → pipeline loads and sidebar updates
```

---

## Recommendation

Close issue #1 as already resolved. Confirm the feature works via the verification steps above, then close.
