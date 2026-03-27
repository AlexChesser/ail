mod app;
mod input;
pub mod theme;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::AppState;

/// Launch the interactive TUI. Returns when the user quits.
pub fn run(pipeline: Option<ail_core::config::domain::Pipeline>) -> io::Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, pipeline);

    // Restore terminal on exit
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    pipeline: Option<ail_core::config::domain::Pipeline>,
) -> io::Result<()> {
    let mut app = AppState::new(pipeline);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Poll for input with a short timeout so the loop stays responsive
        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;
            // Resize events require a redraw — ratatui handles this automatically
            // on the next draw() call, so no special handling needed here.
            input::handle_event(&mut app, ev);
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
