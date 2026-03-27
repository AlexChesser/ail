use ratatui::{
    layout::Alignment,
    style::{Modifier, Style},
    text::Text,
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::AppState;

/// Render the entire TUI for a single frame.
pub fn draw(frame: &mut Frame, _app: &AppState) {
    let version = ail_core::version();
    let text = Text::styled(
        format!("ail v{version}\npress q to quit"),
        Style::default().add_modifier(Modifier::BOLD),
    );
    let paragraph = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(paragraph, frame.area());
}
