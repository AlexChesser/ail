# TASK 1: TUI Implementation - Pipeline Switcher with Autocomplete

## Feature Specification

### TUI Interaction Model
- **`:` (colon alone)**: Display list of available pipelines (top N, sorted by recency or alphabetically)
- **`:a` (colon + letter)**: Filter list to pipelines beginning with that letter
- **`:ab` (colon + multi-char prefix)**: Further filter matching pipelines
- **Navigation**: Up/Down arrow keys to move selection highlight
- **Execution**: Enter key to launch the selected pipeline
- **Cancellation**: Escape key to dismiss the picker without switching

### Display Example
```
[current-pipeline] > :
┌─ Available Pipelines ─────────┐
│ • feature-development         │
│ • incident-debugging          │
│   code-review                 │
│   pre-pr-check                │
│   reflection-self-improvement │
└───────────────────────────────┘

[current-pipeline] > :in
┌─ Available Pipelines ─────────┐
│ • incident-debugging          │
└───────────────────────────────┘

(Press Enter to select, Esc to cancel)
```

---

## Scope

- Autocomplete picker UI (`:` + typeahead + arrow navigation + Enter)
- Pipeline discovery (directory scan or passed-in list)
- Integration with input dispatcher
- Hot-reload invocation (emit command or call runtime)

---

## What This Task Should Design

### 1. Event Loop Integration
- How do keystrokes flow into the picker without blocking the main loop?
- State machine for picker mode: idle → open → filtering → selecting → close

### 2. Data Structure & Filtering
- How is the pipeline list stored and accessed?
- What's the most efficient way to filter on each keystroke?
- Should matches be sorted (recency, alphabetically, by frequency)?

### 3. Rendering Strategy
- Modal overlay on top of REPL? Inline replacement? Side panel?
- How does the picker coexist with the main display?
- Can we use `ratatui` or are we working with lower-level termion?

### 4. Pipeline Discovery Mechanism
- **Option A**: Scan directory at startup (e.g., `~/.ail/pipelines/`)
- **Option B**: Pass list to TUI during initialization
- **Option C**: Query runtime registry
- Trade-offs and recommended approach

### 5. Selection & Invocation
- When Enter is pressed, how does the TUI signal the switch?
- What's the interface between picker and runtime?
- Does it emit a command, call a method, or return a value?

### 6. Error Handling
- What if no pipelines match the filter?
- What if pipelines directory is missing?
- What if a selected pipeline fails to load?
- Should TUI stay in picker, show error inline, or dismiss?

---

## Expected Deliverable

- **Module structure** (e.g., `tui/picker.rs` or `tui/pipeline_switcher.rs`)
- **Key types & functions**:
  - `struct PipelinePickerState { ... }`
  - `fn open_picker() -> Result<PipelinePickerState>`
  - `fn filter_pipelines(prefix: &str) -> Vec<String>`
  - `fn render_picker(state: &PipelinePickerState) -> String`
  - `fn handle_picker_input(event: KeyEvent, state: &mut PipelinePickerState) -> Option<String>`
- **Integration points** with existing TUI input handler
- **No spec changes required**

---

## Blockers

None — this is purely TUI-layer work.

---

## Parallelism

This task can start **immediately** and run **in parallel** with TASK 2 (Spec Impact Analysis). TUI implementation has no dependencies on spec design decisions.

However, TUI design should be **extensible** — it should not preclude the possibility of pipelines emitting switch directives (even if that's decided as "not in scope" right now).

---

## Success Criteria

- [ ] Event loop integration model is specified
- [ ] State machine for picker is defined
- [ ] Pipeline discovery mechanism is chosen (A, B, or C)
- [ ] Rendering strategy is clear (modal, inline, etc.)
- [ ] Selection & invocation interface is designed
- [ ] Error handling paths are mapped
- [ ] Module structure and key types are defined