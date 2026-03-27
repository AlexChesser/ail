use crossterm::event::{Event, KeyCode, KeyModifiers};

use super::app::AppState;

/// Map a crossterm input event to a state change.
pub fn handle_event(app: &mut AppState, event: Event) {
    if let Event::Key(key) = event {
        match (key.modifiers, key.code) {
            // Quit: q or Ctrl-C
            (_, KeyCode::Char('q')) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                app.running = false;
            }
            _ => {}
        }
    }
}
