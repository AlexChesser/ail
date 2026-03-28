use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Apply display styling to a single viewport line.
///
/// Used by the inline mode `insert_before` flusher.
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
