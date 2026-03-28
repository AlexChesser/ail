use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::tui::ui::layout::STATUS_BAR_HEIGHT;

/// Width of the pipeline sidebar in the inline viewport.
pub const SIDEBAR_WIDTH: u16 = 20;

/// The computed Rect regions for the inline chrome panels.
pub struct InlineRegions {
    pub status_bar: Rect,
    pub sidebar: Rect,
    pub prompt: Rect,
}

/// Compute the inline chrome layout within the fixed inline viewport area.
///
/// Status bar occupies the top row (full width). The remaining rows are split
/// horizontally: a 20-col pipeline sidebar on the left and the prompt on the right.
pub fn compute(area: Rect) -> InlineRegions {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(STATUS_BAR_HEIGHT), Constraint::Min(1)])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(1)])
        .split(vertical[1]);

    InlineRegions {
        status_bar: vertical[0],
        sidebar: horizontal[0],
        prompt: horizontal[1],
    }
}
