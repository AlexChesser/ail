use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Width tier thresholds.
const FULL_WIDTH: u16 = 60; // full sidebar with names
const GLYPH_WIDTH: u16 = 40; // sidebar collapses to glyph-only
const STATUS_WIDTH: u16 = 30; // sidebar hidden; step status in status bar

/// Width of the pipeline sidebar in full mode (step names visible).
pub const SIDEBAR_FULL_WIDTH: u16 = 22;
/// Width of the pipeline sidebar in glyph-only mode.
pub const SIDEBAR_GLYPH_WIDTH: u16 = 4;
/// Height of the status bar.
pub const STATUS_BAR_HEIGHT: u16 = 1;
/// Minimum prompt height (border + 1 content line).
const PROMPT_MIN_HEIGHT: u16 = 2;
/// Maximum prompt height in lines of content before it stops expanding.
const PROMPT_MAX_CONTENT_LINES: u16 = 8;

/// Compute dynamic prompt height based on input content and available width.
///
/// `input_chars` is the number of characters in the input buffer.
/// `prefix_len` is the rendered prefix width (e.g. 2 for `"> "`).
/// Returns total area height including the top border row.
pub fn prompt_height(input_chars: usize, prefix_len: u16, available_width: u16) -> u16 {
    let usable = available_width.saturating_sub(prefix_len).max(1) as usize;
    // Number of visual lines: ceil(max(1, input_chars) / usable)
    let content_chars = input_chars.max(1);
    let content_lines = content_chars.div_ceil(usable) as u16;
    let clamped = content_lines.clamp(1, PROMPT_MAX_CONTENT_LINES);
    PROMPT_MIN_HEIGHT - 1 + clamped // border (1) + content lines
}

/// Terminal width tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthTier {
    /// ≥ 120 cols: full layout with pipeline sidebar (names visible)
    Full,
    /// 100–119 cols: sidebar collapses to glyph-only
    GlyphOnly,
    /// 80–99 cols: sidebar hidden; step status in status bar
    NoSidebar,
    /// < 80 cols: minimal mode — output + prompt only
    Minimal,
}

impl WidthTier {
    pub fn from_width(width: u16) -> Self {
        if width >= FULL_WIDTH {
            WidthTier::Full
        } else if width >= GLYPH_WIDTH {
            WidthTier::GlyphOnly
        } else if width >= STATUS_WIDTH {
            WidthTier::NoSidebar
        } else {
            WidthTier::Minimal
        }
    }
}

/// The computed Rect regions for each TUI panel.
pub struct Regions {
    pub sidebar: Option<Rect>,
    pub viewport: Rect,
    pub status_bar: Rect,
    pub prompt: Rect,
}

/// Compute panel regions from the full terminal area.
///
/// `input_chars` is the current number of characters in the prompt input buffer,
/// used to compute a dynamic prompt height.
pub fn compute(area: Rect, input_chars: usize) -> Regions {
    let tier = WidthTier::from_width(area.width);
    let sidebar_width = match tier {
        WidthTier::Full => SIDEBAR_FULL_WIDTH.min(area.width / 2),
        WidthTier::GlyphOnly => SIDEBAR_GLYPH_WIDTH.min(area.width / 2),
        WidthTier::NoSidebar | WidthTier::Minimal => 0,
    };

    // Prompt prefix is "> " (2 chars); compute height based on content length.
    let ph = prompt_height(input_chars, 2, area.width);

    // Split vertically: [content area | status bar | prompt]
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(STATUS_BAR_HEIGHT),
            Constraint::Length(ph),
        ])
        .split(area);

    let content_area = vertical[0];
    let status_bar = vertical[1];
    let prompt = vertical[2];

    // Split content horizontally: [sidebar | viewport]
    let (sidebar, viewport) = if sidebar_width > 0 {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_width), Constraint::Min(1)])
            .split(content_area);
        (Some(horizontal[0]), horizontal[1])
    } else {
        (None, content_area)
    };

    Regions {
        sidebar,
        viewport,
        status_bar,
        prompt,
    }
}
