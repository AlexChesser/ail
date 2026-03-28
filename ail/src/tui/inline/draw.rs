use ratatui::Frame;

use crate::tui::app::AppState;
use crate::tui::ui::{modal, picker, prompt, statusbar};

use super::layout;

/// Render the inline chrome: status bar + prompt (no sidebar, no viewport widget).
///
/// LLM output is flushed above the inline viewport via `terminal.insert_before()` by the
/// event loop — it is not rendered here.
pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();
    let regions = layout::compute(area);

    statusbar::draw(frame, app, regions.status_bar);
    prompt::draw(frame, app, regions.prompt);

    // Picker dropdown — positioned relative to the prompt area (same as fullscreen).
    picker::draw(frame, app, regions.prompt);

    // Modal overlay — centered within the inline viewport area.
    modal::draw(frame, app, area);
}
