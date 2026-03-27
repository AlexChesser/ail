mod app;
mod backend;
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
use backend::{BackendCommand, BackendEvent};

/// Launch the interactive TUI. Returns when the user quits.
pub fn run(
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, pipeline, cli_provider, headless);

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
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
) -> io::Result<()> {
    let mut app = AppState::new(pipeline.clone());
    let (cmd_tx, event_rx) = backend::spawn_backend(pipeline, cli_provider, headless);

    loop {
        // Drain all pending backend events before drawing (non-blocking).
        loop {
            match event_rx.try_recv() {
                Ok(BackendEvent::Executor(ev)) => app.apply_executor_event(ev),
                Ok(BackendEvent::Error(msg)) => {
                    app.viewport_lines.push(format!("[backend error: {msg}]"));
                    app.phase = app::ExecutionPhase::Failed;
                }
                Err(_) => break,
            }
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;
            input::handle_event(&mut app, ev);
        }

        // Submit pending prompt to backend.
        if let Some(prompt) = app.pending_prompt.take() {
            app.echo_prompt(&prompt);
            app.reset_for_run();
            app.phase = app::ExecutionPhase::Running;
            let _ = cmd_tx.send(BackendCommand::SubmitPrompt(prompt));
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
