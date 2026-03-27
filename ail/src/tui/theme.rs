//! Visual language constants — glyphs and colors for pipeline step states.
//! Defined in tui-planning-prompt.md §2.

/// Step state glyphs (§2.1)
#[allow(dead_code)]
pub mod glyphs {
    pub const NOT_REACHED: &str = "○";
    pub const RUNNING: &str = "●";
    pub const COMPLETED: &str = "✓";
    pub const SKIPPED: &str = "⊘";
    pub const DISABLED: &str = "⊖";
    pub const FAILED: &str = "✗";
    pub const HITL: &str = "◉";
    pub const BRANCHED: &str = "◇";
}

/// Step state colors (§2.1)
#[allow(dead_code)]
pub mod colors {
    use ratatui::style::Color;

    pub const NOT_REACHED: Color = Color::DarkGray;
    pub const RUNNING: Color = Color::Cyan;
    pub const COMPLETED: Color = Color::Green;
    pub const SKIPPED: Color = Color::Yellow;
    pub const DISABLED: Color = Color::Magenta;
    pub const FAILED: Color = Color::Red;
    pub const HITL: Color = Color::Yellow;
    pub const BRANCHED: Color = Color::Blue;
}
