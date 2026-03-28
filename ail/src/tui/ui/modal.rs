use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app::AppState;

/// Render any active modal overlay (interrupt modal).
/// Called after all other panels so it renders on top.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    if app.interrupt_modal_open {
        draw_interrupt_modal(frame, app, area);
    }
}

fn draw_interrupt_modal(frame: &mut Frame, app: &AppState, area: Rect) {
    // Center a compact box.
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(area);
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(10),
            Constraint::Percentage(30),
        ])
        .split(horiz[1]);
    let modal_area = vert[1];

    let buf_text: String = app.input_buffer.iter().collect();
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "⏸ PAUSED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Enter     ", Style::default().fg(Color::Green)),
            Span::raw("resume execution"),
        ]),
        Line::from(vec![
            Span::styled("  type+Enter", Style::default().fg(Color::Cyan)),
            Span::raw("  inject guidance for next step"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+K    ", Style::default().fg(Color::Red)),
            Span::raw("  kill current step"),
        ]),
    ];

    if !buf_text.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  guidance: ", Style::default().fg(Color::DarkGray)),
            Span::raw(buf_text),
        ]));
    }

    let block = Block::default()
        .title(" interrupt ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    let para = Paragraph::new(lines).block(block);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(para, modal_area);
}
