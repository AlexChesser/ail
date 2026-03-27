use ail_core::config::domain::{ActionKind, ContextSource, ResultAction, ResultMatcher, StepBody};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app::{AppState, ViewMode};

/// Render any active modal overlay (HITL gate, interrupt modal, step-detail HUD).
/// Called after all other panels so it renders on top.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    match app.view_mode {
        ViewMode::Normal => {}
        ViewMode::StepHud(idx) => draw_step_hud(frame, app, area, idx),
    }
}

fn draw_step_hud(frame: &mut Frame, app: &AppState, area: Rect, step_idx: usize) {
    let pipeline = match &app.pipeline {
        Some(p) => p,
        None => return,
    };
    let step = match pipeline.steps.get(step_idx) {
        Some(s) => s,
        None => return,
    };
    let display = match app.steps.get(step_idx) {
        Some(d) => d,
        None => return,
    };

    // Center a box that takes ~70% width and ~80% height.
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(area);
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(10),
            Constraint::Percentage(80),
            Constraint::Percentage(10),
        ])
        .split(horiz[1]);
    let modal_area = vert[1];

    // Build content lines.
    let mut lines: Vec<Line> = Vec::new();

    let step_type = match &step.body {
        StepBody::Prompt(_) => "prompt",
        StepBody::Skill(_) => "skill",
        StepBody::SubPipeline(_) => "pipeline",
        StepBody::Action(_) => "action",
        StepBody::Context(ContextSource::Shell(_)) => "context: shell",
    };
    lines.push(Line::from(vec![
        Span::styled("type:   ", Style::default().fg(Color::DarkGray)),
        Span::raw(step_type),
    ]));

    if let Some(ref model) = step.model {
        lines.push(Line::from(vec![
            Span::styled("model:  ", Style::default().fg(Color::DarkGray)),
            Span::raw(model.clone()),
        ]));
    }

    lines.push(Line::raw(""));

    // Body detail
    match &step.body {
        StepBody::Prompt(text) | StepBody::Context(ContextSource::Shell(text)) => {
            lines.push(Line::styled(
                "prompt:",
                Style::default().fg(Color::DarkGray),
            ));
            for l in text.lines() {
                lines.push(Line::from(vec![Span::raw("  "), Span::raw(l.to_string())]));
            }
        }
        StepBody::Skill(path) => {
            lines.push(Line::from(vec![
                Span::styled("skill:  ", Style::default().fg(Color::DarkGray)),
                Span::raw(path.display().to_string()),
            ]));
        }
        StepBody::SubPipeline(path) => {
            lines.push(Line::from(vec![
                Span::styled("file:   ", Style::default().fg(Color::DarkGray)),
                Span::raw(path.display().to_string()),
            ]));
        }
        StepBody::Action(ActionKind::PauseForHuman) => {
            lines.push(Line::raw("  pause_for_human"));
        }
    }

    // Tools
    if let Some(ref tools) = step.tools {
        if !tools.allow.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "allowed tools:",
                Style::default().fg(Color::DarkGray),
            ));
            for t in &tools.allow {
                lines.push(Line::from(vec![Span::raw("  "), Span::raw(t.clone())]));
            }
        }
        if !tools.deny.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "denied tools:",
                Style::default().fg(Color::DarkGray),
            ));
            for t in &tools.deny {
                lines.push(Line::from(vec![Span::raw("  "), Span::raw(t.clone())]));
            }
        }
    }

    // on_result branches
    if let Some(ref branches) = step.on_result {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "on_result:",
            Style::default().fg(Color::DarkGray),
        ));
        for b in branches {
            let matcher = match &b.matcher {
                ResultMatcher::Contains(s) => format!("contains \"{s}\""),
                ResultMatcher::ExitCode(ec) => {
                    use ail_core::config::domain::ExitCodeMatch;
                    match ec {
                        ExitCodeMatch::Exact(n) => format!("exit_code {n}"),
                        ExitCodeMatch::Any => "exit_code any".to_string(),
                    }
                }
                ResultMatcher::Always => "always".to_string(),
            };
            let action = match &b.action {
                ResultAction::Continue => "continue",
                ResultAction::Break => "break",
                ResultAction::AbortPipeline => "abort_pipeline",
                ResultAction::PauseForHuman => "pause_for_human",
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(matcher),
                Span::styled(" → ", Style::default().fg(Color::DarkGray)),
                Span::raw(action),
            ]));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Esc/q to close   ↑↓/j k to scroll",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    ));

    // Scroll
    let total = lines.len() as u16;
    let inner_height = modal_area.height.saturating_sub(2); // minus borders
    let max_scroll = total.saturating_sub(inner_height);
    let scroll_y = app.hud_scroll.min(max_scroll);

    let title = format!(" {} ", display.id);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));
    let para = Paragraph::new(lines).block(block).scroll((scroll_y, 0));

    // Clear the area first so the modal doesn't bleed through.
    frame.render_widget(Clear, modal_area);
    frame.render_widget(para, modal_area);
}
