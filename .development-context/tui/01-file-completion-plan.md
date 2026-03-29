# File Completion Feature Plan

> **Feature:** Inline `@`-triggered file browser in the TUI prompt input
> **Reference:** `tui-planning-prompt.md §3.4` — "`@` prefix triggers file path completion"
> **Tracking ID:** `i-2-file-completion` (candidate)

---

## 1. Behavioral Specification

### 1.1 Trigger

Typing `@` anywhere in the prompt input buffer opens the file completion dropdown. Unlike the pipeline picker (which only triggers on `:` in an empty buffer), file completion triggers mid-buffer — `@` is valid at position 0, mid-sentence, or after a space.

The `@` and everything typed after it up to the cursor forms the **completion token** (e.g., `@src/tui/`). On selection, the completion token is replaced in-place with the resolved path.

### 1.2 Navigation Semantics

The dropdown starts in CWD. As the user types after `@`, the input is split into:

- **directory prefix** — everything up to and including the last `/`
- **entry filter** — everything after the last `/`

If the directory prefix changes (i.e., the user types or backspaces past a `/`), the browser re-scans that directory and resets the entry list.

| User types | Browser dir | Entry filter |
|---|---|---|
| `@` | `./` | `` |
| `@src` | `./` | `src` |
| `@src/` | `./src/` | `` |
| `@src/tui` | `./src/` | `tui` |
| `@src/tui/` | `./src/tui/` | `` |

When the user selects a **directory** entry via `Enter`, it appends `/` to the completion token and re-scans — effectively navigating into that directory without closing the popup. When the user selects a **file** entry, the completion token is replaced in the buffer and the popup closes.

### 1.3 Backspace Behavior

- Backspace within the entry filter: delete one character, re-filter.
- Backspace when entry filter is empty and dir prefix is non-empty: pop the last path segment (navigate up), re-scan.
- Backspace when the completion token is just `@` (nothing after it): close and remove the `@` from the buffer.

### 1.4 Escape / Cancellation

`Escape` closes the dropdown without any change to the buffer. The `@` and everything typed after it remains in the buffer exactly as the user left it (i.e., the raw text is preserved — the user typed it, they keep it).

### 1.5 Display

- Dropdown floats above the prompt input area, anchored near the cursor position.
- Shows at most 10 entries. Scrolls if more match.
- Entries are sorted: directories first (with `/` suffix), then files, both groups case-insensitively sorted.
- Hidden files (`.` prefix) are included but shown dimly. They are sorted after non-hidden entries within each group.
- The current browsed directory is shown as the dropdown title, relative to CWD (e.g., ` ./src/tui/ `).
- Entry filter typed after `@` is shown in the title after the path (e.g., ` ./src/tui/ [mod] `).
- Highlighted entry uses the same cyan-bold cursor style as the pipeline picker.
- If the scanned directory has no matching entries, shows `(no matches)` in dim gray.
- If the scanned path does not exist, shows `(not found)` in dim red.

---

## 2. Architecture

### 2.1 New State in `AppState` (`ail/src/tui/app.rs`)

Add a `FileCompletion` block to `AppState` (grouped with a comment like `// File completion (@)`):

```rust
// File completion (@)
/// Whether the file completion dropdown is open.
pub file_comp_open: bool,
/// Byte offset of the `@` character in `input_buffer` (char index).
pub file_comp_at_pos: usize,
/// The directory currently being listed.
pub file_comp_dir: PathBuf,
/// Cached directory entries (names only; dirs have `/` appended).
pub file_comp_entries: Vec<FileCompEntry>,
/// Indices into `file_comp_entries` matching the current filter.
pub file_comp_filtered: Vec<usize>,
/// Cursor row within `file_comp_filtered`.
pub file_comp_cursor: usize,
```

New type (can live in `app.rs` or a new `app/file_comp.rs`):

```rust
#[derive(Debug, Clone)]
pub struct FileCompEntry {
    pub name: String,   // display name, dirs have trailing "/"
    pub is_dir: bool,
}
```

### 2.2 `AppState` methods

All file-completion logic lives on `AppState`. New methods:

| Method | Description |
|---|---|
| `file_comp_open_at(pos: usize)` | Called when `@` is typed. Records `at_pos = pos`, sets `file_comp_dir = cwd`, scans dir, opens popup. |
| `file_comp_scan()` | Re-reads `file_comp_dir`, rebuilds `file_comp_entries`, re-applies filter. Sorts dirs-first then files, hidden last within each group. Errors (dir not found) produce an empty entry list with an error flag. |
| `file_comp_filter_str() -> &str` | Returns the slice of `input_buffer[at_pos+1..]` up to cursor — the text after `@` and after the last `/`. |
| `file_comp_dir_prefix_str() -> String` | Returns the text between `@` and the last `/` in the token, i.e. the directory part. |
| `file_comp_recompute_dir()` | Derives `file_comp_dir` from CWD + `file_comp_dir_prefix_str()`. Calls `file_comp_scan()`. |
| `file_comp_apply_filter()` | Rebuilds `file_comp_filtered` by case-insensitive prefix match on `file_comp_filter_str()`. Resets `file_comp_cursor`. |
| `file_comp_type_char(c: char)` | Inserts `c` into the input buffer at cursor, then calls `file_comp_recompute_dir()` if `c == '/'` or `file_comp_apply_filter()` otherwise. |
| `file_comp_backspace()` | Deletes char before cursor; re-derives dir/filter; calls `file_comp_recompute_dir()` if dir changed, else `file_comp_apply_filter()`. Closes if cursor moves before `at_pos`. |
| `file_comp_nav_up()` / `file_comp_nav_down()` | Move `file_comp_cursor` within `file_comp_filtered`. |
| `file_comp_select()` | Selects the highlighted entry. If dir: append `/` to buffer, call `file_comp_recompute_dir()`. If file: replace `@<token>` in buffer with the full relative path, close. |
| `file_comp_close()` | Sets `file_comp_open = false`, zeroes other fields. Does NOT modify the buffer. |

### 2.3 Input handling (`ail/src/tui/input.rs`)

Add a new intercept guard, analogous to `picker_open`, in `handle_event`:

```rust
if app.file_comp_open {
    handle_file_completion(app, key.modifiers, key.code);
    return;
}
```

This guard sits **after** `perm_modal_open`, `interrupt_modal_open`, and `picker_open` checks — file completion is lower priority than modals.

New function `handle_file_completion`:

| Key | Action |
|---|---|
| `Ctrl+C` | `app.running = false` |
| `Escape` | `app.file_comp_close()` |
| `Enter` | `app.file_comp_select()` |
| `Up` / `k` | `app.file_comp_nav_up()` |
| `Down` / `j` | `app.file_comp_nav_down()` |
| `Backspace` | `app.file_comp_backspace()` |
| `Char(c)` | `app.file_comp_type_char(c)` |

In `handle_prompt`, modify the `Char(c)` branch: when `c == '@'`, after inserting the character into the buffer, call `app.file_comp_open_at(app.cursor_pos - 1)`.

### 2.4 Rendering (`ail/src/tui/ui/`)

Add `ail/src/tui/ui/file_completion.rs`. The `draw` function mirrors `picker::draw`:

- Takes `frame`, `app`, and `prompt_area: Rect`.
- Computes popup position: same logic as picker (above prompt, left-aligned), but anchor X is offset to align with the `@` symbol's column in the prompt line rather than always `+2`.
- Width: `60.min(prompt_area.width - 4)` — wider than pipeline picker because paths are longer.
- Height: `min(visible_count, 10) + 2` (borders).
- Title: `" ./relative/dir/ [filter] "` or just `" ./ "` when filter is empty.
- Entries: dirs rendered in blue, files in default color; hidden entries dim.
- `(no matches)` and `(not found)` error states.

Register in `ail/src/tui/ui/mod.rs` and call from the top-level draw function after `picker::draw`.

### 2.5 No changes to `ail-core`

File system reads use `std::fs::read_dir` directly in `app.rs`. This is TUI-only logic. The `ail-core` boundary is not crossed.

---

## 3. Edge Cases and Constraints

| Case | Handling |
|---|---|
| `@` typed when picker is open | Picker takes precedence (picker intercept fires first); `@` is typed into picker filter normally. |
| `@` typed during pipeline run | File completion is allowed during `ExecutionPhase::Running`. The input buffer is active only during `Idle`, `Completed`, `Failed`, `HitlGate`. If input is disabled, the `@` char won't reach `handle_prompt`. No special handling needed. |
| Large directories (> 500 entries) | Cap `file_comp_entries` at 500 entries after sorting. Show `(truncated — type to filter)` hint when list was capped. |
| Symlinks | Treat symlinks to directories as directories. Follow one level. |
| Permission errors on `read_dir` | Show `(permission denied)` in dim red. Close after `Escape`. |
| Relative vs absolute paths in output | Always insert paths relative to CWD (e.g., `./src/tui/mod.rs`), not absolute. This matches Claude Code `@` convention. |
| Multiple `@` tokens in one buffer | The most recently typed `@` (i.e., the one at `at_pos`) is the active one. Earlier `@` tokens in the buffer are inert text. |
| Cursor moves before `at_pos` via `Left` arrow | `file_comp_close()` is called because the completion token is abandoned. |
| Tab completion (future) | Not in scope for this feature. `Tab` is not mapped. |
| CWD unavailable | Degrade gracefully: show `(cwd unavailable)`, close on any keypress. |

---

## 4. Implementation Order

1. **`app.rs`** — add `FileCompEntry`, state fields, and all `file_comp_*` methods. Unit-testable in isolation.
2. **`input.rs`** — add `handle_file_completion`, add `@` trigger in `handle_prompt`.
3. **`ui/file_completion.rs`** — rendering, wired into draw loop.
4. **`ui/mod.rs`** — call `file_completion::draw` after picker.
5. Manual smoke test: `cargo run -- --once "test" --pipeline demo/.ail.yaml`, type `@`, navigate, select, verify buffer replacement.
6. Edge case sweep: large dir, permission error, `@` mid-sentence, Escape preserves buffer.

---

## 5. Files Changed

| File | Change type |
|---|---|
| `ail/src/tui/app.rs` | Add state fields + methods |
| `ail/src/tui/input.rs` | Add intercept + `@` trigger |
| `ail/src/tui/ui/file_completion.rs` | New file — renderer |
| `ail/src/tui/ui/mod.rs` | Register + call new renderer |

No changes to `ail-core`, `spec/`, or `RUNNER-SPEC.md`. No `ail-core/CLAUDE.md` update needed (TUI-only change).

---

## 6. Open Questions

1. **Scroll offset anchoring** — should the dropdown anchor to the column of the `@` sign in the rendered prompt, or always left-align? Left-align is simpler and consistent with the picker; column-anchoring is more precise but requires computing character-to-pixel offset in ratatui, which is non-trivial with wrapping.
   _Proposed default: left-align, revisit if it feels awkward during testing._

2. **`@` in the middle of a word** — if the user types `foo@bar`, should `@` trigger completion? The spec says "`@` prefix" which implies a word boundary (preceded by space or at buffer start). Should we require that `@` is preceded by a space or is at position 0?
   _Proposed default: require `@` to be at position 0 or preceded by a space/newline. `foo@bar` does not trigger completion — `@` is inserted as a literal character._

3. **Aborting completion when the cursor leaves the token** — detecting that the user moved the cursor leftward past `at_pos` requires hooking every cursor-movement action to re-check. Is it worth the complexity, or is `Escape` sufficient?
   _Proposed default: hook `cursor_left`, `cursor_home`, `cursor_word_left` to close if `cursor_pos` would go <= `at_pos`._
