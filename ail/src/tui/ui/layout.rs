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
/// Height of the prompt input area.
pub const PROMPT_HEIGHT: u16 = 3;

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
pub fn compute(area: Rect) -> Regions {
    let tier = WidthTier::from_width(area.width);
    let sidebar_width = match tier {
        WidthTier::Full => SIDEBAR_FULL_WIDTH.min(area.width / 2),
        WidthTier::GlyphOnly => SIDEBAR_GLYPH_WIDTH.min(area.width / 2),
        WidthTier::NoSidebar | WidthTier::Minimal => 0,
    };

    // Split vertically: [content area | status bar | prompt]
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(STATUS_BAR_HEIGHT),
            Constraint::Length(PROMPT_HEIGHT),
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
