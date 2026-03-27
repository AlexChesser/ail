use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::AppState;

/// Render the status bar.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let _ = app; // will be populated in M6
    let line = Line::from(vec![
        Span::styled("○ ail", Style::default().fg(Color::DarkGray)),
        Span::raw(" | idle"),
    ]);
    let para = Paragraph::new(line).style(Style::default().bg(Color::Reset));
    frame.render_widget(para, area);
}
