use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::{
    app::{AppState, Focus, StepGlyph},
    theme::{colors, glyphs},
};

fn glyph_for(state: StepGlyph) -> &'static str {
    match state {
        StepGlyph::NotReached => glyphs::NOT_REACHED,
        StepGlyph::Running => glyphs::RUNNING,
        StepGlyph::Completed => glyphs::COMPLETED,
        StepGlyph::Failed => glyphs::FAILED,
        StepGlyph::Skipped => glyphs::SKIPPED,
        StepGlyph::Disabled => glyphs::DISABLED,
        StepGlyph::HitlPaused => glyphs::HITL,
    }
}

fn color_for(state: StepGlyph) -> Color {
    match state {
        StepGlyph::NotReached => colors::NOT_REACHED,
        StepGlyph::Running => colors::RUNNING,
        StepGlyph::Completed => colors::COMPLETED,
        StepGlyph::Failed => colors::FAILED,
        StepGlyph::Skipped => colors::SKIPPED,
        StepGlyph::Disabled => colors::DISABLED,
        StepGlyph::HitlPaused => colors::HITL,
    }
}

/// Render the pipeline sidebar.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect, glyph_only: bool) {
    let focused = app.focus == Focus::Sidebar;
    let border_color = if focused {
        Color::White
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::RIGHT)
        .style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = if app.steps.is_empty() {
        vec![Line::from(Span::styled(
            "no pipeline",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let glyph = glyph_for(step.glyph);
                let color = color_for(step.glyph);
                let cursor_active = focused && i == app.sidebar_cursor;
                let base_style = if cursor_active {
                    Style::default().fg(color).add_modifier(Modifier::REVERSED)
                } else {
                    Style::default().fg(color)
                };
                if glyph_only {
                    Line::from(Span::styled(glyph, base_style))
                } else {
                    Line::from(vec![
                        Span::styled(glyph, base_style),
                        Span::styled(" ", base_style),
                        Span::styled(&step.id, base_style),
                    ])
                }
            })
            .collect()
    };

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}
