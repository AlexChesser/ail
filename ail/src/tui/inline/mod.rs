mod draw;
mod layout;

use std::io;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::{
    event::{
        self, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    widgets::{Paragraph, Widget, Wrap},
    Terminal, TerminalOptions, Viewport,
};

use super::app::{AppState, ExecutionPhase, PromptAction, SideEffect};
use super::backend::{BackendCommand, BackendEvent};
use super::ui::viewport::style_line;

/// Height of the inline viewport in terminal rows.
///
/// Accommodates: 1 status bar + 9 rows for sidebar/prompt.
const INLINE_HEIGHT: u16 = 10;

/// Execute side effects returned by pure state-mutation methods.
fn execute_effects(effects: &[SideEffect], app: &AppState) {
    for effect in effects {
        match effect {
            SideEffect::SendPermissionResponse(resp) => {
                if let Some(ref tx) = app.permissions.tx {
                    let _ = tx.send(resp.clone());
                }
            }
            SideEffect::SetPauseFlag(val) => {
                if let Some(ref f) = app.interrupt.pause_flag {
                    f.store(*val, Ordering::SeqCst);
                }
            }
            SideEffect::SetKillFlag => {
                if let Some(ref f) = app.interrupt.kill_flag {
                    f.store(true, Ordering::SeqCst);
                }
            }
        }
    }
}

/// Launch the primary-buffer TUI. Returns when the user quits.
///
/// LLM output flows into the terminal's native primary-buffer scrollback via
/// `terminal.insert_before()`. The status bar, pipeline sidebar, and prompt are
/// rendered inside the fixed inline viewport at the bottom of the terminal.
pub fn run(
    pipeline: Option<ail_core::config::domain::Pipeline>,
    cli_provider: ail_core::config::domain::ProviderConfig,
    runner: Box<dyn ail_core::runner::Runner + Send>,
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

    run_app(&mut terminal, pipeline, cli_provider, runner)?;

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
    runner: Box<dyn ail_core::runner::Runner + Send>,
) -> io::Result<()> {
    let mut app = AppState::new(pipeline.clone());
    app.picker.entries = ail_core::config::discovery::discover_all();
    let (cmd_tx, event_rx) = super::backend::spawn_backend(pipeline, cli_provider, runner);

    let mut hitl_tx: Option<mpsc::Sender<String>> = None;
    // Index into app.viewport.lines of the next line to flush to the scrollback.
    let mut last_flushed: usize = 0;

    loop {
        // Drain all pending backend events before drawing (non-blocking).
        loop {
            match event_rx.try_recv() {
                Ok(BackendEvent::Executor(ev)) => app.apply_executor_event(ev),
                Ok(BackendEvent::Error(msg)) => {
                    app.viewport.lines.push(format!("[backend error: {msg}]"));
                    app.phase = ExecutionPhase::Failed;
                }
                Ok(BackendEvent::HitlReady(tx)) => {
                    hitl_tx = Some(tx);
                }
                Ok(BackendEvent::ControlReady { pause, kill }) => {
                    app.interrupt.pause_flag = Some(pause);
                    app.interrupt.kill_flag = Some(kill);
                }
                Ok(BackendEvent::PermReady(tx)) => {
                    app.permissions.tx = Some(tx);
                }
                Ok(BackendEvent::PermissionRequest(req)) => {
                    let effects = app.handle_permission_request(req);
                    execute_effects(&effects, &app);
                }
                Err(_) => break,
            }
        }

        // Flush new viewport lines into the primary-buffer scrollback.
        // Wrap 4 columns short of the terminal's right edge to leave a visual gutter.
        let current = app.viewport.lines.len();
        let term_width = terminal.size()?.width;
        let wrap_width = term_width.saturating_sub(4).max(1);
        for i in last_flushed..current {
            let line_text = app.viewport.lines[i].clone();
            let char_count = line_text.chars().count() as u16;
            let rows = if char_count == 0 {
                1
            } else {
                char_count.div_ceil(wrap_width)
            };
            terminal.insert_before(rows, |buf| {
                let render_area = Rect {
                    width: wrap_width.min(buf.area.width),
                    ..buf.area
                };
                Paragraph::new(style_line(&line_text))
                    .wrap(Wrap { trim: false })
                    .render(render_area, buf);
            })?;
        }
        last_flushed = current;
        terminal.draw(|f| draw::draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;
            let effects = super::input::handle_event(&mut app, ev);
            execute_effects(&effects, &app);
        }

        // Submit pending prompt to backend (or unblock HITL gate).
        match app.resolve_pending_prompt() {
            PromptAction::SendHitl(prompt) => {
                if let Some(ref tx) = hitl_tx {
                    let _ = tx.send(prompt);
                }
            }
            PromptAction::SubmitToBackend {
                prompt,
                disabled_steps,
            } => {
                let _ = cmd_tx.send(BackendCommand::SubmitPrompt {
                    prompt,
                    disabled_steps,
                });
            }
            PromptAction::None => {}
        }

        // Hot-reload: apply a pending pipeline switch.
        if let Some(path) = app.picker.pending_pipeline_switch.take() {
            match ail_core::config::load(&path) {
                Ok(new_pipeline) => {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    let cmd_pipeline = app.apply_pipeline_switch(new_pipeline, name);
                    let _ = cmd_tx.send(BackendCommand::SwitchPipeline(cmd_pipeline));
                }
                Err(e) => {
                    app.apply_pipeline_switch_error(&e.detail);
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
