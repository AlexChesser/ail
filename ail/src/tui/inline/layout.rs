use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::tui::ui::layout::STATUS_BAR_HEIGHT;

/// The computed Rect regions for the inline chrome panels.
pub struct InlineRegions {
    pub status_bar: Rect,
    pub prompt: Rect,
}

/// Compute the inline chrome layout within the fixed inline viewport area.
///
/// Status bar occupies the top row; the prompt gets all remaining rows.
pub fn compute(area: Rect) -> InlineRegions {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(STATUS_BAR_HEIGHT), Constraint::Min(1)])
        .split(area);

    InlineRegions {
        status_bar: vertical[0],
        prompt: vertical[1],
    }
}
