mod draw;
mod layout;

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::{
    event::{
        self, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    backend::CrosstermBackend,
    widgets::{Paragraph, Widget},
    Terminal, TerminalOptions, Viewport,
};

use super::app::{AppState, ExecutionPhase};
use super::backend::{BackendCommand, BackendEvent};
use super::ui::viewport::style_line;

/// Height of the inline viewport in terminal rows.
///
/// Accommodates: 1 status bar + 9 rows for sidebar/prompt.
const INLINE_HEIGHT: u16 = 10;

/// Launch the primary-buffer TUI. Returns when the user quits.
///
/// LLM output flows into the terminal's native primary-buffer scrollback via
/// `terminal.insert_before()`. The status bar, pipeline sidebar, and prompt are
/// rendered inside the fixed inline viewport at the bottom of the terminal.
pub fn run(
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Start in the primary buffer — no EnterAlternateScreen, no mouse capture.
    // Not capturing mouse events lets the terminal handle text selection natively.
    let keyboard_enhanced = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
    .is_ok();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(INLINE_HEIGHT),
        },
    )?;

    run_app(&mut terminal, pipeline, cli_provider, headless)?;

    disable_raw_mode()?;
    if keyboard_enhanced {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    terminal.show_cursor()?;
    // Print a newline so the shell prompt appears on a fresh line.
    println!();

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    headless: bool,
) -> io::Result<()> {
    let mut app = AppState::new(pipeline.clone());
    app.picker_entries = ail_core::config::discovery::discover_all();
    let (cmd_tx, event_rx) = super::backend::spawn_backend(pipeline, cli_provider, headless);

    let mut hitl_tx: Option<mpsc::Sender<String>> = None;
    // Index into app.viewport_lines of the next line to flush to the scrollback.
    let mut last_flushed: usize = 0;

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
                Ok(BackendEvent::PermReady(tx)) => {
                    app.perm_tx = Some(tx);
                }
                Ok(BackendEvent::PermissionRequest(req)) => {
                    app.handle_permission_request(req);
                }
                Err(_) => break,
            }
        }

        // Flush new viewport lines into the primary-buffer scrollback.
        let current = app.viewport_lines.len();
        for i in last_flushed..current {
            let line_text = app.viewport_lines[i].clone();
            terminal.insert_before(1, |buf| {
                Paragraph::new(style_line(&line_text)).render(buf.area, buf);
            })?;
        }
        last_flushed = current;
        terminal.draw(|f| draw::draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;
            super::input::handle_event(&mut app, ev);
        }

        // Submit pending prompt to backend (or unblock HITL gate).
        if let Some(prompt) = app.pending_prompt.take() {
            if app.phase == ExecutionPhase::HitlGate {
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

        // Hot-reload: apply a pending pipeline switch.
        if let Some(path) = app.pending_pipeline_switch.take() {
            match ail_core::config::load(&path) {
                Ok(new_pipeline) => {
                    app.steps = AppState::steps_for_pipeline(&new_pipeline);
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
                }
                Err(e) => {
                    app.viewport_lines
                        .push(format!("[pipeline load error: {}]", e.detail));
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
