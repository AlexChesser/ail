use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};
use crate::tui::theme::{colors, glyphs};

/// Format a token count with K/M/B suffix. One decimal when transitioning to a new magnitude.
fn fmt_tokens(n: u64) -> String {
    if n < 1_000 {
        format!("{n}")
    } else if n < 10_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else if n < 1_000_000 {
        format!("{}K", n / 1_000)
    } else if n < 10_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n < 1_000_000_000 {
        format!("{}M", n / 1_000_000)
    } else {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    }
}

/// Render the status bar with live execution stats (M6).
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let (glyph, glyph_color) = match app.phase {
        ExecutionPhase::Idle => (glyphs::NOT_REACHED, colors::NOT_REACHED),
        ExecutionPhase::Running => (glyphs::RUNNING, colors::RUNNING),
        ExecutionPhase::Paused => (glyphs::HITL, colors::HITL),
        ExecutionPhase::HitlGate => (glyphs::HITL, colors::HITL),
        ExecutionPhase::Completed => (glyphs::COMPLETED, colors::COMPLETED),
        ExecutionPhase::Failed => (glyphs::FAILED, colors::FAILED),
    };

    let dim = Style::default().fg(Color::DarkGray);
    let hi = Style::default().fg(Color::White);

    let mut spans: Vec<Span> = vec![Span::styled(
        format!("{glyph} ail"),
        Style::default().fg(glyph_color),
    )];

    match app.phase {
        ExecutionPhase::Running | ExecutionPhase::Paused => {
            // ▶ ail | step 2/4: review | $0.0032 | 1,847 tok | 12.4s
            if app.total_steps > 0 {
                let step_name = app.active_step_id.as_deref().unwrap_or("?");
                spans.push(Span::styled(
                    format!(
                        " | step {}/{}: {}",
                        app.current_step_index + 1,
                        app.total_steps,
                        step_name
                    ),
                    hi,
                ));
            }
            if app.cumulative_input_tokens > 0 || app.cumulative_output_tokens > 0 {
                spans.push(Span::styled(
                    format!(
                        " | [↑{} | ↓{}]",
                        fmt_tokens(app.cumulative_input_tokens),
                        fmt_tokens(app.cumulative_output_tokens)
                    ),
                    dim,
                ));
            }
            if let Some(start) = app.run_start {
                let secs = start.elapsed().as_secs_f32();
                spans.push(Span::styled(format!(" | {secs:.1}s"), dim));
            }
            if app.phase == ExecutionPhase::Paused {
                spans.push(Span::styled(
                    " | PAUSED",
                    Style::default().fg(Color::Yellow),
                ));
            }
        }
        ExecutionPhase::HitlGate => {
            let step = app.active_step_id.as_deref().unwrap_or("?");
            spans.push(Span::styled(
                format!(" | HITL — step: {step}"),
                Style::default().fg(Color::Yellow),
            ));
        }
        ExecutionPhase::Completed => {
            // ✓ ail | [↑1.2K | ↓3.4K] | 12.4s
            if app.cumulative_input_tokens > 0 || app.cumulative_output_tokens > 0 {
                spans.push(Span::styled(
                    format!(
                        " | [↑{} | ↓{}]",
                        fmt_tokens(app.cumulative_input_tokens),
                        fmt_tokens(app.cumulative_output_tokens)
                    ),
                    dim,
                ));
            }
            if let (Some(start), Some(end)) = (app.run_start, app.run_end) {
                let secs = (end - start).as_secs_f32();
                spans.push(Span::styled(format!(" | {secs:.1}s"), dim));
            }
        }
        ExecutionPhase::Failed => {
            spans.push(Span::styled(" | failed", Style::default().fg(Color::Red)));
        }
        ExecutionPhase::Idle => {
            // ○ ail | pipeline: code-review | idle | session: abc123
            // Show active pipeline name for context (i-1).
            let pipeline_name = app
                .pipeline
                .as_ref()
                .and_then(|p| p.source.as_ref())
                .and_then(|s| s.file_stem())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "passthrough".to_string());
            spans.push(Span::styled(
                format!(" | {pipeline_name}"),
                Style::default().fg(Color::White),
            ));
            spans.push(Span::styled(" | idle", dim));
            if let Some(ref sid) = app.last_session_id {
                let short = &sid[..sid.len().min(8)];
                spans.push(Span::styled(format!(" | session: {short}"), dim));
            }
            if app.total_steps > 0 {
                spans.push(Span::styled(format!(" | {} steps", app.total_steps), dim));
            }
        }
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line).style(Style::default().bg(Color::Reset));
    frame.render_widget(para, area);
}
