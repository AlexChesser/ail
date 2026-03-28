use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};

/// Apply display styling to a single viewport line.
///
/// Used by both the fullscreen viewport widget and the inline mode `insert_before` flusher.
pub fn style_line(line: &str) -> Line<'static> {
    if (line.starts_with("── ") && line.ends_with(" ──"))
        || line == "── done ──"
        || line.starts_with("> ")
    {
        let style = if line.starts_with("> ") {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Line::from(Span::styled(line.to_owned(), style))
    } else if line.starts_with("[thinking] ") {
        Line::from(Span::styled(
            line.to_owned(),
            Style::default().fg(Color::DarkGray),
        ))
    } else if line.starts_with("  [tool: ") {
        Line::from(Span::styled(
            line.to_owned(),
            Style::default().fg(Color::Magenta),
        ))
    } else if line.starts_with("[error") || line.starts_with("[pipeline error") {
        Line::from(Span::styled(
            line.to_owned(),
            Style::default().fg(Color::Red),
        ))
    } else {
        Line::raw(line.to_owned())
    }
}

/// Render the main viewport. Updates `app.viewport_height` and `app.viewport_width` from the area each frame.
pub fn draw(frame: &mut Frame, app: &mut AppState, area: Rect) {
    app.viewport_height = area.height;
    app.viewport_width = area.width;

    let source_lines = app.active_viewport_lines();

    let lines: Vec<Line> = if source_lines.is_empty() {
        // Idle placeholder
        let hint = match app.phase {
            ExecutionPhase::Idle => {
                let version = ail_core::version();
                format!("ail v{version} — type a prompt and press Enter")
            }
            ExecutionPhase::Running => "running...".to_string(),
            _ => String::new(),
        };
        vec![Line::styled(
            hint,
            Style::default().add_modifier(Modifier::DIM),
        )]
    } else {
        source_lines.iter().map(|l| style_line(l)).collect()
    };

    // Count visual (wrapped) lines so scroll math matches Paragraph::scroll() behaviour.
    let width = area.width.max(1) as usize;
    let total: u16 = lines
        .iter()
        .map(|l| {
            let w = l.width();
            if w == 0 { 1 } else { ((w - 1) / width + 1) as u16 }
        })
        .sum();
    let visible = area.height;
    // scroll_y = how many visual lines to skip from the top.
    // When auto-scrolling (viewport_scroll == 0), show the bottom.
    let max_from_top = total.saturating_sub(visible);
    let scroll_y = max_from_top.saturating_sub(app.viewport_scroll);

    // Add viewing indicator when browsing a historical step (M9).
    let para = if let Some(idx) = app.viewing_step {
        let step_id = app.step_order.get(idx).map(|s| s.as_str()).unwrap_or("?");
        let total_steps = app.step_order.len();
        let indicator = format!(
            " viewing: {step_id} ({}/{total_steps}) — Ctrl+N for next, Ctrl+N again for live ",
            idx + 1
        );
        Paragraph::new(lines)
            .scroll((scroll_y, 0))
            .wrap(Wrap { trim: false })
            .block(
                ratatui::widgets::Block::default()
                    .title(Span::styled(indicator, Style::default().fg(Color::Yellow)))
                    .borders(ratatui::widgets::Borders::NONE),
            )
    } else {
        Paragraph::new(lines)
            .scroll((scroll_y, 0))
            .wrap(Wrap { trim: false })
    };

    frame.render_widget(para, area);

    // Scrollbar: only render when content overflows the viewport.
    if total > visible {
        let mut scrollbar_state = ScrollbarState::new(total as usize)
            .viewport_content_length(visible as usize)
            .position(scroll_y as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}
