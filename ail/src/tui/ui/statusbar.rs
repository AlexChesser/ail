use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};
use crate::tui::theme::{colors, glyphs};

/// Render the status bar.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let (glyph, phase_text, glyph_color) = match app.phase {
        ExecutionPhase::Idle => (glyphs::NOT_REACHED, "idle", colors::NOT_REACHED),
        ExecutionPhase::Running => (glyphs::RUNNING, "running", colors::RUNNING),
        ExecutionPhase::Completed => (glyphs::COMPLETED, "done", colors::COMPLETED),
        ExecutionPhase::Failed => (glyphs::FAILED, "failed", colors::FAILED),
    };

    let line = Line::from(vec![
        Span::styled(format!("{glyph} ail"), Style::default().fg(glyph_color)),
        Span::styled(
            format!(" | {phase_text}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let para = Paragraph::new(line).style(Style::default().bg(Color::Reset));
    frame.render_widget(para, area);
}
