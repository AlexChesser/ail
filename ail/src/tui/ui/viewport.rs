use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};

/// Render the main viewport. Updates `app.viewport_height` from the area each frame.
pub fn draw(frame: &mut Frame, app: &mut AppState, area: Rect) {
    app.viewport_height = area.height;

    let lines: Vec<Line> = if app.viewport_lines.is_empty() {
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
        app.viewport_lines
            .iter()
            .map(|l| {
                // Step separators rendered in dim style.
                if (l.starts_with("── ") && l.ends_with(" ──"))
                    || l == "── done ──"
                    || l.starts_with("> ")
                {
                    let style = if l.starts_with("> ") {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    Line::from(Span::styled(l.clone(), style))
                } else if l.starts_with("  [tool: ") {
                    Line::from(Span::styled(l.clone(), Style::default().fg(Color::Magenta)))
                } else if l.starts_with("[error") || l.starts_with("[pipeline error") {
                    Line::from(Span::styled(l.clone(), Style::default().fg(Color::Red)))
                } else {
                    Line::raw(l.clone())
                }
            })
            .collect()
    };

    let total = lines.len() as u16;
    let visible = area.height;
    // scroll_y = how many lines to skip from the top.
    // When auto-scrolling (viewport_scroll == 0), show the bottom.
    let max_from_top = total.saturating_sub(visible);
    let scroll_y = max_from_top.saturating_sub(app.viewport_scroll);

    let para = Paragraph::new(lines).scroll((scroll_y, 0));
    frame.render_widget(para, area);
}
