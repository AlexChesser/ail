pub mod layout;
pub mod modal;
pub mod picker;
pub mod prompt;
pub mod sidebar;
pub mod statusbar;
pub mod viewport;

use ratatui::Frame;

use crate::tui::app::AppState;
use layout::WidthTier;

/// Render the entire TUI for a single frame.
pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();
    let regions = layout::compute(area, app.input_buffer.len());
    let tier = WidthTier::from_width(area.width);

    // Sidebar (visible in Full and GlyphOnly tiers)
    if let Some(sidebar_area) = regions.sidebar {
        sidebar::draw(frame, app, sidebar_area, tier == WidthTier::GlyphOnly);
    }

    // Main viewport (updates app.viewport_height)
    viewport::draw(frame, app, regions.viewport);

    // Status bar
    statusbar::draw(frame, app, regions.status_bar);

    // Prompt input
    prompt::draw(frame, app, regions.prompt);

    // Pipeline picker dropdown (above prompt, below modal) — drawn after prompt
    picker::draw(frame, app, regions.prompt);

    // Modal overlay (HITL gate, interrupt, HUD) — drawn last so it's on top
    modal::draw(frame, app, regions.viewport);
}
