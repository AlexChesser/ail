use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use super::app::{AppState, ExecutionPhase};

/// Map a crossterm input event to a state change.
pub fn handle_event(app: &mut AppState, event: Event) {
    if let Event::Key(key) = event {
        // With keyboard enhancement active the terminal emits Press, Repeat, and Release
        // events. Only act on Press and Repeat; ignore Release to avoid double-fires.
        if key.kind == KeyEventKind::Release {
            return;
        }
        // Permission modal intercepts all input when a tool permission check is pending.
        if app.permissions.modal_open {
            handle_permission_modal(app, key.modifiers, key.code);
            return;
        }
        // Interrupt modal intercepts input when paused.
        if app.interrupt.modal_open {
            handle_interrupt_modal(app, key.modifiers, key.code);
            return;
        }
        // Picker intercepts when open (i-1).
        if app.picker.open {
            handle_picker(app, key.modifiers, key.code);
            return;
        }
        handle_prompt(app, key.modifiers, key.code);
    }
}

fn handle_picker(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.running = false;
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            // No-op if filter matches nothing.
            app.picker.select();
        }
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            app.picker.nav_up();
        }
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            app.picker.nav_down();
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.picker.backspace();
        }
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            app.picker.type_char(c);
        }
        _ => {}
    }
}

fn handle_permission_modal(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        // Letter shortcuts (unchanged)
        (KeyModifiers::NONE, KeyCode::Char('y')) => app.perm_approve_once(),
        (KeyModifiers::NONE, KeyCode::Char('a')) => app.perm_approve_session(),
        (KeyModifiers::NONE, KeyCode::Char('n')) => app.perm_deny(),
        // Arrow navigation
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            app.permissions.nav_up();
        }
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            app.permissions.nav_down();
        }
        // Enter confirms the highlighted option
        (KeyModifiers::NONE, KeyCode::Enter) => app.perm_confirm(),
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.perm_deny();
            app.running = false;
        }
        _ => {}
    }
}

fn handle_interrupt_modal(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        // Path C: Ctrl+K = kill step
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            app.request_kill();
        }
        // Path B: type guidance then Enter to inject and resume
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if !app.prompt.input_buffer.is_empty() {
                let text: String = app.prompt.input_buffer.iter().collect();
                app.prompt.input_buffer.clear();
                app.prompt.cursor_pos = 0;
                app.request_inject_guidance(text);
            } else {
                // Empty Enter = resume (same as Escape)
                app.request_resume();
            }
        }
        // Allow typing guidance while paused
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            app.prompt.input_insert(c);
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.prompt.input_backspace();
        }
        (KeyModifiers::NONE, KeyCode::Delete) => {
            app.prompt.input_delete();
        }
        (KeyModifiers::NONE, KeyCode::Left) => app.prompt.cursor_left(),
        (KeyModifiers::NONE, KeyCode::Right) => app.prompt.cursor_right(),
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.running = false;
        }
        _ => {}
    }
}

fn handle_prompt(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        // Quit: Ctrl-C
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.running = false;
        }

        // Ctrl+K: kill step directly (no pause first) (M11)
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            if app.phase == ExecutionPhase::Running {
                app.request_kill();
            }
        }

        // Submit prompt (or unblock HITL gate — empty Enter is valid there)
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if app.phase == ExecutionPhase::HitlGate && app.prompt.input_buffer.is_empty() {
                // Empty Enter = approve/continue
                app.prompt.pending_prompt = Some(String::new());
            } else {
                app.prompt.submit_input();
            }
        }

        // Shift+Enter or Alt+Enter inserts a newline in the buffer.
        (KeyModifiers::SHIFT | KeyModifiers::ALT, KeyCode::Enter) => {
            app.prompt.input_insert('\n');
        }

        // Printable characters
        // `:` alone in empty buffer opens the pipeline picker (i-1).
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            if c == ':'
                && app.prompt.input_buffer.is_empty()
                && !app.picker.entries.is_empty()
                && matches!(
                    app.phase,
                    ExecutionPhase::Idle | ExecutionPhase::Completed | ExecutionPhase::Failed
                )
            {
                app.picker.open_picker();
            } else {
                app.prompt.input_insert(c);
            }
        }

        // Editing
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.prompt.input_backspace();
        }
        (KeyModifiers::NONE, KeyCode::Delete) => {
            app.prompt.input_delete();
        }

        // Cursor movement
        (KeyModifiers::NONE, KeyCode::Left) => {
            app.prompt.cursor_left();
        }
        (KeyModifiers::NONE, KeyCode::Right) => {
            app.prompt.cursor_right();
        }
        (KeyModifiers::NONE, KeyCode::Home) => {
            app.prompt.cursor_home();
        }
        (KeyModifiers::NONE, KeyCode::End) => {
            app.prompt.cursor_end();
        }

        // Word jump: Ctrl+Left / Ctrl+Right
        (KeyModifiers::CONTROL, KeyCode::Left) => {
            app.prompt.cursor_word_left();
        }
        (KeyModifiers::CONTROL, KeyCode::Right) => {
            app.prompt.cursor_word_right();
        }

        // Session navigation (M9)
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.viewport.session_prev();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            app.viewport.session_next();
        }

        // Up: navigate up within multiline prompt; fall through to history on first line.
        (KeyModifiers::NONE, KeyCode::Up) => {
            if !app.prompt.cursor_up_line() {
                app.prompt.history_up();
            }
        }
        // Down: navigate down within multiline prompt; fall through to history on last line.
        (KeyModifiers::NONE, KeyCode::Down) => {
            if !app.prompt.cursor_down_line() {
                app.prompt.history_down();
            }
        }

        _ => {}
    }
}
