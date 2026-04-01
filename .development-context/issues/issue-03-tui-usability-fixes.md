# Issue #3: TUI Usability Fixes

## Context
8 usability issues discovered during local TUI testing. Each fix is a discrete commit. After codebase exploration, 7 of 8 are already implemented — only Fix 8 (sidebar breakpoints) requires work.

---

## Fix Status Summary

| # | Title | Status | File | Line(s) |
|---|-------|--------|------|---------|
| 1 | Disable mouse capture | ✓ Done | `ail/src/tui/inline/mod.rs` | 67–68 |
| 2 | Redirect tracing to stderr | ✓ Done | `ail/src/main.rs` | 12–32 |
| 3 | Enable text wrapping in viewport | ✓ Done | `ail/src/tui/inline/mod.rs` | 154–155 |
| 4 | Fix scrollback during streaming | ✓ Done | `ail/src/tui/app.rs` | 478–822 |
| 5 | Replace cost display with token counters | ✓ Done | `ail/src/tui/ui/statusbar.rs` | 13–27, 49–107 |
| 6 | TUI backend missing invocation step | ✓ Done | `ail/src/tui/backend.rs`, `ail/src/tui/app.rs` | 111–179, 631–657 |
| 7 | Streaming feedback — thinking blocks | ✓ Done | `ail/src/tui/app.rs`, `ail/src/tui/ui/viewport.rs` | 757–762, 20–24 |
| 8 | Lower sidebar breakpoints | **TODO** | `ail/src/tui/inline/layout.rs` | 6, 19–28 |

---

## Fix 8: Lower Sidebar Breakpoints (ONLY REMAINING WORK)

**File:** `ail/src/tui/inline/layout.rs:6`

**Problem:** Sidebar disappears at terminal widths below ~100 columns. The constant `SIDEBAR_WIDTH: u16 = 20` is too wide relative to breakpoint logic, making the sidebar vanish on narrower terminals.

**Change:** Make sidebar width responsive to terminal width:

```rust
// In compute() function, replace static SIDEBAR_WIDTH with:
let sidebar_width = (area.width / 2).max(12).min(30);
```

Or lower the constant if breakpoint logic is the issue:
```rust
pub const SIDEBAR_WIDTH: u16 = 12;  // was 20
```

**Steps:**
1. Read `ail/src/tui/inline/layout.rs` lines 1–40 to understand full breakpoint logic
2. Determine if the issue is the constant or the breakpoint condition
3. Apply the appropriate fix
4. Test at widths: 30, 40, 50, 80, 120 columns

**Build & verify:**
```bash
cargo build
cargo clippy -- -D warnings
cargo nextest run
```

---

## Notes on Already-Implemented Fixes (for reference)

- **Fix 1** — `mod.rs:67–68` explicitly avoids `EnableMouseCapture` with a comment: "no mouse capture. Not capturing mouse events lets the terminal handle text selection natively."
- **Fix 2** — `main.rs:14–26` routes TUI logs to `~/.ail/tui.log`; non-TUI to stderr.
- **Fix 3** — `inline/mod.rs:154–155` uses `.wrap(Wrap { trim: false })` on the paragraph widget.
- **Fix 4** — `app.rs` `StreamDelta` and `ToolUse` handlers explicitly do NOT reset scroll (comments in code); `append_text()` at lines 478–490 compensates for added lines when `scroll > 0`.
- **Fix 5** — `statusbar.rs:13–27` `fmt_tokens()` formats `[↑1.2K | ↓3.4K]`.
- **Fix 6** — `backend.rs:111–179` checks for missing invocation step and emits synthetic `StepStarted`; `app.rs:631–657` prepends "invocation" to sidebar.
- **Fix 7** — `app.rs:757–762` prefixes thinking lines with `[thinking] `; `viewport.rs:20–24` renders them in `Color::DarkGray`.

---

## Verification Checklist (post Fix 8)

- [ ] Text is selectable/copyable in terminal
- [ ] No log lines corrupt TUI display during streaming
- [ ] Long lines wrap within viewport
- [ ] Scrolling up during streaming preserves position; new step resets to bottom
- [ ] Status bar shows `[↑1.2K | ↓3.4K]` format
- [ ] Sidebar shows "invocation" step for pipelines without explicit invocation
- [ ] Thinking lines appear dim gray with `[thinking] ` prefix
- [ ] Sidebar visible and readable at 30–50 column terminal widths
