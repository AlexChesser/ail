use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};

/// Render the main viewport.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let version = ail_core::version();

    let text = match &app.last_response {
        Some(resp) => {
            let style = match app.phase {
                ExecutionPhase::Failed => Style::default().fg(Color::Red),
                _ => Style::default(),
            };
            Text::styled(resp.clone(), style)
        }
        None => {
            let hint = match app.phase {
                ExecutionPhase::Idle => format!("ail v{version}\nwaiting for prompt..."),
                ExecutionPhase::Running => "running...".to_string(),
                ExecutionPhase::Completed => "done.".to_string(),
                ExecutionPhase::Failed => "pipeline failed.".to_string(),
            };
            Text::styled(hint, Style::default().add_modifier(Modifier::DIM))
        }
    };

    let para = Paragraph::new(text)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}
