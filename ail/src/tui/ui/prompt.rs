use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::AppState;

/// Render the prompt input area.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let _ = app; // will be populated in M3
    let block = Block::default()
        .borders(Borders::TOP)
        .style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::raw(""),
    ]);
    let para = Paragraph::new(line);
    frame.render_widget(para, inner);
}
