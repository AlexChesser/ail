use crossterm::event::{Event, KeyCode, KeyModifiers};

use super::app::{AppState, Focus};

/// Map a crossterm input event to a state change.
pub fn handle_event(app: &mut AppState, event: Event) {
    if let Event::Key(key) = event {
        match app.focus {
            Focus::Sidebar => handle_sidebar(app, key.modifiers, key.code),
            Focus::Prompt => handle_prompt(app, key.modifiers, key.code),
        }
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

        // Submit prompt
        (KeyModifiers::NONE, KeyCode::Enter) => {
            app.submit_input();
        }

        // Shift+Enter inserts a newline in the buffer
        (KeyModifiers::SHIFT, KeyCode::Enter) => {
            app.input_insert('\n');
        }

        // Printable characters
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            app.input_insert(c);
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
