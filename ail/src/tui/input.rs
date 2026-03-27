use crossterm::event::{Event, KeyCode, KeyModifiers};

use super::app::AppState;

/// Map a crossterm input event to a state change.
pub fn handle_event(app: &mut AppState, event: Event) {
    if let Event::Key(key) = event {
        match (key.modifiers, key.code) {
            // Quit: Ctrl-C (q removed — q is a valid prompt character)
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                app.running = false;
            }

            // Submit prompt
            (KeyModifiers::NONE, KeyCode::Enter) => {
                app.submit_input();
            }

            // Character insertion (Shift+Enter inserts a newline in the buffer)
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
}
