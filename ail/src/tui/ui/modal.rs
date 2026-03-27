use ratatui::{layout::Rect, Frame};

use crate::tui::app::AppState;

/// Render any active modal overlay (HITL gate, interrupt modal, step-detail HUD).
/// Called after all other panels so it renders on top.
pub fn draw(_frame: &mut Frame, _app: &AppState, _area: Rect) {
    // Populated in M8 (HUD), M10 (HITL gate), M11 (interrupt modal).
}
