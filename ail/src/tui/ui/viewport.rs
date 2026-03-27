use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Text,
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::AppState;

/// Render the main viewport.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let _ = app; // will be used in M4+
    let version = ail_core::version();
    let text = Text::styled(
        format!("ail v{version}\nwaiting for prompt..."),
        Style::default().add_modifier(Modifier::DIM),
    );
    let para = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(para, area);
}
