mod app;
mod backend;
mod inline;
mod input;
pub mod theme;
mod ui;

use std::io;

/// Launch the interactive TUI. Returns when the user quits.
pub fn run(
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
) -> io::Result<()> {
    inline::run(pipeline, cli_provider, headless)
}
