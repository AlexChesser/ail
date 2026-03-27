use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::AppState;

/// Render the pipeline sidebar.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect, glyph_only: bool) {
    let _ = app; // will be used in M2
    let block = Block::default()
        .borders(Borders::RIGHT)
        .style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let placeholder = if glyph_only {
        "○\n○\n○"
    } else {
        "○ (pipeline)\n○ (steps)\n○ (here)"
    };
    let lines: Vec<Line> = placeholder
        .lines()
        .map(|l| Line::from(Span::raw(l)))
        .collect();
    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}
