mod app;
mod backend;
mod fullscreen;
mod inline;
mod input;
pub mod theme;
mod ui;

use std::io;

/// TUI rendering mode.
pub enum TuiMode {
    /// Full-screen alternate screen buffer (default). Provides sidebar, viewport widget, and all
    /// overlay features. The terminal's native scrollbar is disabled while the TUI is running.
    Fullscreen,
    /// Inline viewport at the bottom of the primary buffer. LLM output flows into native terminal
    /// scrollback; only the status bar and prompt are rendered in the fixed inline area.
    Inline,
}

/// Launch the interactive TUI. Returns when the user quits.
pub fn run(
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
    mode: TuiMode,
) -> io::Result<()> {
    match mode {
        TuiMode::Fullscreen => fullscreen::run(pipeline, cli_provider, headless),
        TuiMode::Inline => inline::run(pipeline, cli_provider, headless),
    }
}
