mod app;
mod backend;
mod input;
pub mod theme;
mod ui;

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::{
    event,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{AppState, ExecutionPhase, StepDisplay, StepGlyph};
use backend::{BackendCommand, BackendEvent};

/// Launch the interactive TUI. Returns when the user quits.
pub fn run(
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, pipeline, cli_provider, headless);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
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
    app.picker_entries = ail_core::config::discovery::discover_all();
    let (cmd_tx, event_rx) = backend::spawn_backend(pipeline, cli_provider, headless);

    // HITL gate sender: received from backend when a run starts, used to unblock PauseForHuman.
    let mut hitl_tx: Option<mpsc::Sender<String>> = None;

    loop {
        // Drain all pending backend events before drawing (non-blocking).
        loop {
            match event_rx.try_recv() {
                Ok(BackendEvent::Executor(ev)) => app.apply_executor_event(ev),
                Ok(BackendEvent::Error(msg)) => {
                    app.viewport_lines.push(format!("[backend error: {msg}]"));
                    app.phase = ExecutionPhase::Failed;
                }
                Ok(BackendEvent::HitlReady(tx)) => {
                    hitl_tx = Some(tx);
                }
                Ok(BackendEvent::ControlReady { pause, kill }) => {
                    app.pause_flag = Some(pause);
                    app.kill_flag = Some(kill);
                }
                Err(_) => break,
            }
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;
            input::handle_event(&mut app, ev);
        }

        // Submit pending prompt to backend (or unblock HITL gate).
        if let Some(prompt) = app.pending_prompt.take() {
            if app.phase == ExecutionPhase::HitlGate {
                // HITL gate: send the response (may be empty — means "continue")
                if let Some(ref tx) = hitl_tx {
                    let _ = tx.send(prompt);
                }
                app.phase = ExecutionPhase::Running;
            } else {
                app.echo_prompt(&prompt);
                app.reset_for_run();
                app.phase = ExecutionPhase::Running;
                let _ = cmd_tx.send(BackendCommand::SubmitPrompt {
                    prompt,
                    disabled_steps: app.disabled_steps.clone(),
                });
            }
        }

        // Hot-reload: apply a pending pipeline switch (i-1).
        if let Some(path) = app.pending_pipeline_switch.take() {
            match ail_core::config::load(&path) {
                Ok(new_pipeline) => {
                    app.steps = new_pipeline
                        .steps
                        .iter()
                        .map(|s| StepDisplay {
                            id: s.id.as_str().to_string(),
                            glyph: StepGlyph::NotReached,
                        })
                        .collect();
                    app.pipeline = Some(new_pipeline.clone());
                    app.sidebar_cursor = 0;
                    app.disabled_steps.clear();
                    let _ = cmd_tx.send(BackendCommand::SwitchPipeline(new_pipeline));
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    app.viewport_lines
                        .push(format!("── switched to: {name} ──"));
                    app.viewport_scroll = 0;
                }
                Err(e) => {
                    app.viewport_lines
                        .push(format!("[pipeline load error: {}]", e.detail));
                    app.viewport_scroll = 0;
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
