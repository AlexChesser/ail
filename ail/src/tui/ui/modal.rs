use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app::AppState;

/// Render any active modal overlay. Called after all other panels so it renders on top.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    // Permission modal has highest priority.
    if app.permissions.modal_open {
        draw_permission_modal(frame, app, area);
        return;
    }
    if app.interrupt.modal_open {
        draw_interrupt_modal(frame, app, area);
    }
}

fn draw_permission_modal(frame: &mut Frame, app: &AppState, area: Rect) {
    // In the inline 10-row TUI there are no vertical margins — we need the full height to
    // show all options without clipping. Horizontal margins are kept for aesthetics.
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(5),
            Constraint::Fill(1),
            Constraint::Percentage(5),
        ])
        .split(area);
    let modal_area = horiz[1];

    // Compact header — details already written to primary buffer scrollback above.
    let tool_name = app
        .permissions
        .request
        .as_ref()
        .map(|r| r.display_name.as_str())
        .unwrap_or("?");
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
            Span::styled(
                tool_name.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " — permission required",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
    ];

    // Render the three options; highlight the one at permissions.cursor.
    let options: &[(usize, &str, &str, Color)] = &[
        (0, "y", "approve once", Color::Green),
        (1, "a", "allow for session", Color::Cyan),
        (2, "n", "deny", Color::Red),
    ];
    for &(idx, key, label, color) in options {
        let selected = idx == app.permissions.cursor;
        let prefix = if selected { "▶ " } else { "  " };
        let row_style = if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::styled(format!("{prefix}{key}  {label}"), row_style));
        // Overlay the key letter in its color regardless of selection highlight.
        let last = lines.last_mut().unwrap();
        *last = Line::from(vec![
            Span::styled(
                prefix.to_string(),
                if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                key.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {label}"),
                if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
        ]);
    }

    let block = Block::default()
        .title(" permission check ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    let para = Paragraph::new(lines).block(block);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(para, modal_area);
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

    let buf_text: String = app.prompt.input_buffer.iter().collect();
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
