use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};

/// Render the prompt input area with the live input buffer and cursor.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // While the picker is open, show `:filter` with a trailing block cursor (i-1).
    let line = if app.picker_open {
        let filter = &app.picker_filter;
        let cursor_char = " "; // always append cursor at end of filter
        Line::from(vec![
            Span::styled(": ", Style::default().fg(Color::Cyan)),
            Span::raw(filter.clone()),
            Span::styled(
                cursor_char,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        // Build spans: prefix, text-before-cursor, cursor char (highlighted), text-after-cursor
        let buf = &app.input_buffer;
        let cursor = app.cursor_pos.min(buf.len());

        let before: String = buf[..cursor].iter().collect();
        let cursor_char: String = if cursor < buf.len() {
            buf[cursor..=cursor].iter().collect()
        } else {
            " ".to_string() // block cursor at end
        };
        let after: String = if cursor + 1 < buf.len() {
            buf[cursor + 1..].iter().collect()
        } else {
            String::new()
        };

        let (prefix, prefix_color) = if app.phase == ExecutionPhase::HitlGate {
            ("◉ ", Color::Yellow)
        } else {
            ("> ", Color::Cyan)
        };

        Line::from(vec![
            Span::styled(prefix, Style::default().fg(prefix_color)),
            Span::raw(before),
            Span::styled(
                cursor_char,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(after),
        ])
    };

    let para = Paragraph::new(line);
    frame.render_widget(para, inner);
}
