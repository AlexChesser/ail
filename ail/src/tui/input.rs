use crossterm::event::{Event, KeyCode, KeyModifiers};

use super::app::{AppState, ExecutionPhase, Focus, ViewMode};

/// Map a crossterm input event to a state change.
pub fn handle_event(app: &mut AppState, event: Event) {
    if let Event::Key(key) = event {
        // HUD overlay intercepts all input when open.
        if app.view_mode != ViewMode::Normal {
            handle_hud(app, key.modifiers, key.code);
            return;
        }
        // Interrupt modal intercepts input when paused.
        if app.interrupt_modal_open {
            handle_interrupt_modal(app, key.modifiers, key.code);
            return;
        }
        // Picker intercepts when open (i-1).
        if app.picker_open {
            handle_picker(app, key.modifiers, key.code);
            return;
        }
        match app.focus {
            Focus::Sidebar => handle_sidebar(app, key.modifiers, key.code),
            Focus::Prompt => handle_prompt(app, key.modifiers, key.code),
        }
    }
}

fn handle_picker(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.running = false;
        }
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.close_picker();
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            // No-op if filter matches nothing.
            app.picker_select();
        }
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            app.picker_nav_up();
        }
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            app.picker_nav_down();
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.picker_backspace();
        }
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            app.picker_type_char(c);
        }
        _ => {}
    }
}

fn handle_interrupt_modal(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        // Path A: Escape = resume
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.request_resume();
        }
        // Path C: Ctrl+K = kill step
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            app.request_kill();
        }
        // Path B: type guidance then Enter to inject and resume
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if !app.input_buffer.is_empty() {
                let text: String = app.input_buffer.iter().collect();
                app.input_buffer.clear();
                app.cursor_pos = 0;
                app.request_inject_guidance(text);
            } else {
                // Empty Enter = resume (same as Escape)
                app.request_resume();
            }
        }
        // Allow typing guidance while paused
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            app.input_insert(c);
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.input_backspace();
        }
        (KeyModifiers::NONE, KeyCode::Delete) => {
            app.input_delete();
        }
        (KeyModifiers::NONE, KeyCode::Left) => app.cursor_left(),
        (KeyModifiers::NONE, KeyCode::Right) => app.cursor_right(),
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.running = false;
        }
        _ => {}
    }
}

fn handle_hud(app: &mut AppState, _modifiers: KeyModifiers, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.hud_close();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.hud_scroll_up();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.hud_scroll_down();
        }
        _ => {}
    }
}

fn handle_sidebar(app: &mut AppState, modifiers: KeyModifiers, code: KeyCode) {
    match (modifiers, code) {
        // Global quit
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            app.running = false;
        }
        // Return focus to prompt
        (KeyModifiers::NONE, KeyCode::Esc)
        | (KeyModifiers::NONE, KeyCode::Tab)
        | (KeyModifiers::NONE, KeyCode::Char('q')) => {
            app.sidebar_exit_focus();
        }
        // Navigate
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            app.sidebar_nav_up();
        }
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            app.sidebar_nav_down();
        }
        // Toggle disabled
        (KeyModifiers::NONE, KeyCode::Char(' ')) => {
            app.sidebar_toggle_disabled();
        }
        // Open step-detail HUD
        (KeyModifiers::NONE, KeyCode::Enter) => {
            app.hud_open();
        }
        // Viewport scroll still available
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            app.viewport_page_up();
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            app.viewport_page_down();
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

        // Tab: move focus to sidebar
        (KeyModifiers::NONE, KeyCode::Tab) => {
            app.sidebar_enter_focus();
        }

        // Viewport scroll (global — available regardless of focus)
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            app.viewport_page_up();
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            app.viewport_page_down();
        }

        // Escape during Running: request pause and show interrupt modal (M11)
        (KeyModifiers::NONE, KeyCode::Esc) => {
            if app.phase == ExecutionPhase::Running {
                app.request_pause();
            }
        }

        // Ctrl+K: kill step directly (no pause first) (M11)
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            if app.phase == ExecutionPhase::Running {
                app.request_kill();
            }
        }

        // Submit prompt (or unblock HITL gate — empty Enter is valid there)
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if app.phase == ExecutionPhase::HitlGate && app.input_buffer.is_empty() {
                // Empty Enter = approve/continue
                app.pending_prompt = Some(String::new());
            } else {
                app.submit_input();
            }
        }

        // Shift+Enter inserts a newline in the buffer
        (KeyModifiers::SHIFT, KeyCode::Enter) => {
            app.input_insert('\n');
        }

        // Printable characters
        // `:` alone in empty buffer opens the pipeline picker (i-1).
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            if c == ':'
                && app.input_buffer.is_empty()
                && !app.picker_entries.is_empty()
                && matches!(
                    app.phase,
                    ExecutionPhase::Idle | ExecutionPhase::Completed | ExecutionPhase::Failed
                )
            {
                app.open_picker();
            } else {
                app.input_insert(c);
            }
        }

        // Editing
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.input_backspace();
        }
        (KeyModifiers::NONE, KeyCode::Delete) => {
            app.input_delete();
        }

        // Cursor movement
        (KeyModifiers::NONE, KeyCode::Left) => {
            app.cursor_left();
        }
        (KeyModifiers::NONE, KeyCode::Right) => {
            app.cursor_right();
        }
        (KeyModifiers::NONE, KeyCode::Home) => {
            app.cursor_home();
        }
        (KeyModifiers::NONE, KeyCode::End) => {
            app.cursor_end();
        }

        // Word jump: Ctrl+Left / Ctrl+Right
        (KeyModifiers::CONTROL, KeyCode::Left) => {
            app.cursor_word_left();
        }
        (KeyModifiers::CONTROL, KeyCode::Right) => {
            app.cursor_word_right();
        }

        // Session navigation (M9)
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.session_prev();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            app.session_next();
        }

        // History navigation
        (KeyModifiers::NONE, KeyCode::Up) => {
            app.history_up();
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            app.history_down();
        }

        _ => {}
    }
}
