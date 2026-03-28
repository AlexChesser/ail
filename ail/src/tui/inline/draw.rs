use ratatui::Frame;

use crate::tui::app::AppState;
use crate::tui::ui::{modal, picker, prompt, sidebar, statusbar};

use super::layout;

/// Render the inline chrome: status bar + pipeline sidebar + prompt.
///
/// LLM output is flushed above the inline viewport via `terminal.insert_before()` by the
/// event loop — it is not rendered here.
pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();
    let regions = layout::compute(area);

    statusbar::draw(frame, app, regions.status_bar);
    sidebar::draw(frame, app, regions.sidebar, false);
    prompt::draw(frame, app, regions.prompt);

    // Picker dropdown — positioned relative to the prompt area.
    picker::draw(frame, app, regions.prompt);

    // Modal overlay — centered within the inline viewport area.
    modal::draw(frame, app, area);
}
