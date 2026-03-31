use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{AppState, ExecutionPhase};

/// Render the prompt input area with the live input buffer and cursor.
pub fn draw(frame: &mut Frame, app: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cursor_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);

    // While the picker is open, show `:filter` with a trailing block cursor (i-1).
    let lines: Vec<Line> = if app.picker.open {
        let filter = &app.picker.filter;
        vec![Line::from(vec![
            Span::styled(": ", Style::default().fg(Color::Cyan)),
            Span::raw(filter.clone()),
            Span::styled(" ", cursor_style),
        ])]
    } else {
        let buf = &app.prompt.input_buffer;
        let cursor = app.prompt.cursor_pos.min(buf.len());

        let (prefix, prefix_color) = if app.phase == ExecutionPhase::HitlGate {
            ("◉ ", Color::Yellow)
        } else {
            ("> ", Color::Cyan)
        };

        // Split the buffer on newlines to produce one ratatui Line per logical line.
        // Track which logical line the cursor falls on so we can render the block cursor.
        let logical_lines: Vec<&[char]> = buf.split(|&c| c == '\n').collect();
        let mut char_offset = 0usize;
        let mut result: Vec<Line> = Vec::with_capacity(logical_lines.len());

        for (i, seg) in logical_lines.iter().enumerate() {
            let seg_start = char_offset;
            let seg_end = char_offset + seg.len();
            // cursor is between seg_start and seg_end (inclusive of end for last char position)
            let cursor_in_seg = cursor >= seg_start && cursor <= seg_end;

            if cursor_in_seg {
                let local = cursor - seg_start;
                let before: String = seg[..local].iter().collect();
                let cursor_char: String = if local < seg.len() {
                    seg[local..=local].iter().collect()
                } else {
                    " ".to_string()
                };
                let after: String = if local + 1 < seg.len() {
                    seg[local + 1..].iter().collect()
                } else {
                    String::new()
                };

                let mut spans = Vec::with_capacity(5);
                if i == 0 {
                    spans.push(Span::styled(prefix, Style::default().fg(prefix_color)));
                }
                spans.push(Span::raw(before));
                spans.push(Span::styled(cursor_char, cursor_style));
                spans.push(Span::raw(after));
                result.push(Line::from(spans));
            } else {
                let text: String = seg.iter().collect();
                let mut spans = Vec::with_capacity(2);
                if i == 0 {
                    spans.push(Span::styled(prefix, Style::default().fg(prefix_color)));
                }
                spans.push(Span::raw(text));
                result.push(Line::from(spans));
            }

            // +1 to skip the '\n' separator between segments
            char_offset = seg_end + 1;
        }

        result
    };

    // Compute scroll offset so the cursor line is always visible.
    // Paragraph renders from the top; if cursor is below the visible area we scroll down.
    let scroll: u16 = if app.picker.open || inner.height == 0 {
        0
    } else {
        let usable_first = (inner.width as usize).saturating_sub(2).max(1);
        let usable_rest = (inner.width as usize).max(1);
        let buf = &app.prompt.input_buffer;
        let logical_lines: Vec<&[char]> = buf.split(|&c| c == '\n').collect();
        let cursor = app.prompt.cursor_pos.min(buf.len());
        // Recompute which logical line the cursor is on and the local col.
        let mut char_offset = 0usize;
        let mut c_logical = 0usize;
        let mut c_local = 0usize;
        for (i, seg) in logical_lines.iter().enumerate() {
            let seg_start = char_offset;
            let seg_end = char_offset + seg.len();
            if cursor >= seg_start && cursor <= seg_end {
                c_logical = i;
                c_local = cursor - seg_start;
                break;
            }
            char_offset = seg_end + 1;
        }
        // Count visual rows from top up to (and including) the cursor's visual row.
        let mut vrow: u16 = 0;
        for (i, seg) in logical_lines.iter().enumerate() {
            let usable = if i == 0 { usable_first } else { usable_rest };
            if i < c_logical {
                vrow += (seg.len().max(1)).div_ceil(usable) as u16;
            } else {
                // Cursor's logical line: count only up to cursor column.
                vrow += (c_local / usable) as u16;
                break;
            }
        }
        vrow.saturating_sub(inner.height.saturating_sub(1))
    };

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(para, inner);
}
