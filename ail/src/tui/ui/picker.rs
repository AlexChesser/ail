use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app::AppState;

/// Render the pipeline-picker dropdown above the prompt area (i-1).
/// No-op when the picker is not open.
pub fn draw(frame: &mut Frame, app: &AppState, prompt_area: Rect) {
    if !app.picker_open {
        return;
    }

    let visible_count = app.picker_filtered.len().min(8);
    // +2 for top/bottom borders; at least 3 lines tall (borders + 1 content line).
    let height = (visible_count as u16 + 2).max(3);
    let width = (40u16).min(prompt_area.width.saturating_sub(4));

    // Position directly above the prompt, left-aligned after the "> " prefix.
    let x = prompt_area.x + 2;
    let y = prompt_area.y.saturating_sub(height);

    let picker_area = Rect {
        x,
        y,
        width,
        height,
    };

    // Build content lines.
    let lines: Vec<Line> = if app.picker_filtered.is_empty() {
        vec![Line::from(Span::styled(
            "  (no matches)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ))]
    } else {
        app.picker_filtered
            .iter()
            .enumerate()
            .map(|(i, &entry_idx)| {
                let entry = &app.picker_entries[entry_idx];
                let cursor = i == app.picker_cursor;
                let bullet = if cursor { "• " } else { "  " };
                let style = if cursor {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(bullet, style),
                    Span::styled(entry.name.clone(), style),
                ])
            })
            .collect()
    };

    let title = if app.picker_filter.is_empty() {
        " Pipelines ".to_string()
    } else {
        format!(" :{} ", app.picker_filter)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::DarkGray));
    let para = Paragraph::new(lines).block(block);

    frame.render_widget(Clear, picker_area);
    frame.render_widget(para, picker_area);
}
