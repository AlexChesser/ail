mod ask_user_hook;
mod ask_user_types;
mod chat;
mod check_permission_hook;
mod cli;
mod delete;
mod log;
mod logs;

use ail_core::runner::factory::RunnerFactory;
use ail_core::runner::{InvokeOptions, Runner};
use clap::Parser;
use cli::{Cli, Commands, OutputFormat};

/// Discover and load a pipeline from an optional explicit path, falling back to
/// automatic discovery and then passthrough mode. Exits with code 1 on load error.
fn load_pipeline(explicit_path: Option<std::path::PathBuf>) -> ail_core::config::domain::Pipeline {
    match ail_core::config::discovery::discover(explicit_path) {
        Some(ref path) => match ail_core::config::load(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        None => ail_core::config::domain::Pipeline::passthrough(),
    }
}

/// Initialise tracing. Always writes structured JSON logs to stderr.
fn init_tracing() {
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .init();
}

/// Run `--once` / positional prompt with human-readable text output (default lean mode).
///
/// When `show_thinking` or `watch` is set, uses streaming execution to print
/// per-step progress, thinking blocks, and/or responses as they arrive.
fn run_once_text(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    prompt: &str,
    show_thinking: bool,
    watch: bool,
    show_work: bool,
) {
    let run_start = std::time::Instant::now();

    let has_invocation_step = session.has_invocation_step();

    if !has_invocation_step {
        let options = InvokeOptions {
            model: session.cli_provider.model.clone(),
            extensions: runner.build_extensions(&session.cli_provider),
            ..InvokeOptions::default()
        };
        match ail_core::executor::run_invocation_step(session, runner, prompt, options) {
            Ok(result) => {
                if watch {
                    println!(
                        "[1/{}] invocation ({} in / {} out)",
                        session.pipeline.steps.len() + 1,
                        result.input_tokens,
                        result.output_tokens
                    );
                    println!("\n  [Response]\n{}\n", result.response);
                }
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }

    if show_thinking || watch {
        run_once_text_verbose(session, runner, show_thinking, watch);
    } else if show_work {
        run_once_text_show_work(session, runner, has_invocation_step, run_start);
    } else {
        run_once_text_quiet(session, runner, has_invocation_step, run_start);
    }
}

/// Lean/quiet path: no per-step output, just print the final response(s), with a subtle footer.
fn run_once_text_quiet(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    has_invocation_step: bool,
    run_start: std::time::Instant,
) {
    match ail_core::executor::execute(session, runner) {
        Ok(outcome) => {
            use ail_core::executor::ExecuteOutcome;
            if let ExecuteOutcome::Break { step_id } = outcome {
                tracing::info!(event = "pipeline_break", step_id = %step_id);
            }
            session.turn_log.record_run_finished("completed");
            if has_invocation_step {
                if let Some(resp) = session.turn_log.response_for_step("invocation") {
                    println!("{resp}");
                }
            }
            if let Some(entry) = session
                .turn_log
                .entries()
                .iter()
                .rev()
                .find(|e| e.step_id != "invocation" && e.response.is_some())
            {
                println!(
                    "\n--- {} ---\n{}",
                    entry.step_id,
                    entry.response.as_deref().unwrap_or("")
                );
            }

            // Lean footer: only when stdout is a TTY and pipeline had steps (not passthrough).
            let non_invocation_steps = session
                .turn_log
                .entries()
                .iter()
                .filter(|e| e.step_id != "invocation")
                .count();
            if non_invocation_steps > 0 && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                let elapsed = run_start.elapsed().as_secs_f64();
                println!("[ail: {non_invocation_steps} steps in {elapsed:.1}s]");
            }
        }
        Err(e) => {
            session.turn_log.record_run_finished("failed");
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// Show-work summary mode: print one line per completed step after execution.
fn run_once_text_show_work(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    _has_invocation_step: bool,
    run_start: std::time::Instant,
) {
    match ail_core::executor::execute(session, runner) {
        Ok(outcome) => {
            use ail_core::executor::ExecuteOutcome;
            if let ExecuteOutcome::Break { step_id } = outcome {
                tracing::info!(event = "pipeline_break", step_id = %step_id);
            }
            session.turn_log.record_run_finished("completed");

            let non_invocation: Vec<_> = session
                .turn_log
                .entries()
                .iter()
                .filter(|e| e.step_id != "invocation")
                .collect();

            if !non_invocation.is_empty() {
                println!("[pipeline]");
                for entry in &non_invocation {
                    let snippet = entry
                        .response
                        .as_deref()
                        .or(entry.stdout.as_deref())
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim();
                    let snippet = if snippet.len() > 80 {
                        format!("{}…", &snippet[..79])
                    } else {
                        snippet.to_string()
                    };
                    println!("✓ {}   — {}", entry.step_id, snippet);
                }
                let elapsed = run_start.elapsed().as_secs_f64();
                println!("[ail: {} steps in {elapsed:.1}s]", non_invocation.len());
            } else {
                // Passthrough: print invocation response directly.
                if let Some(resp) = session.turn_log.response_for_step("invocation") {
                    println!("{resp}");
                }
            }
        }
        Err(e) => {
            session.turn_log.record_run_finished("failed");
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// Verbose/watch path: print per-step progress + optional thinking/response blocks.
///
/// Uses `execute_with_control` so `RunnerEvent::Thinking` and `RunnerEvent::StreamDelta`
/// events are available for display. The unbounded mpsc channel means execute_with_control
/// never blocks; events are drained after it returns.
fn run_once_text_verbose(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    show_thinking: bool,
    watch: bool,
) {
    use ail_core::executor::{ExecutionControl, ExecutorEvent};
    use ail_core::runner::RunnerEvent;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;

    let (event_tx, event_rx) = mpsc::channel();
    let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let control = ExecutionControl {
        pause_requested: Arc::new(AtomicBool::new(false)),
        kill_requested: Arc::new(AtomicBool::new(false)),
        permission_responder: None,
    };
    let disabled_steps = HashSet::new();

    let result = ail_core::executor::execute_with_control(
        session,
        runner,
        &control,
        &disabled_steps,
        event_tx,
        hitl_rx,
    );

    // Drain events (event_tx dropped when execute_with_control returned, so iter terminates).
    let mut thinking_buf = String::new();
    let mut response_buf = String::new();

    for event in event_rx.iter() {
        match event {
            ExecutorEvent::StepStarted {
                step_id,
                step_index,
                total_steps,
                ..
            } => {
                thinking_buf.clear();
                response_buf.clear();
                eprintln!(
                    "[{}/{}] {} — running...",
                    step_index + 1,
                    total_steps,
                    step_id
                );
            }
            ExecutorEvent::StepCompleted {
                step_id,
                input_tokens,
                output_tokens,
                ..
            } => {
                eprintln!(
                    "    ✓ {} ({} in / {} out)",
                    step_id, input_tokens, output_tokens
                );
                if show_thinking && !thinking_buf.is_empty() {
                    eprintln!("\n  [Thinking]\n{}\n", thinking_buf.trim_end());
                    thinking_buf.clear();
                }
                if watch && !response_buf.is_empty() {
                    eprintln!("\n  [Response]\n{}\n", response_buf.trim_end());
                    response_buf.clear();
                }
            }
            ExecutorEvent::StepFailed { step_id, error } => {
                eprintln!("    ✗ {}: {}", step_id, error);
            }
            ExecutorEvent::RunnerEvent { event: re } => match re {
                RunnerEvent::Thinking { text } => {
                    thinking_buf.push_str(&text);
                }
                RunnerEvent::StreamDelta { text } => {
                    response_buf.push_str(&text);
                }
                _ => {}
            },
            _ => {}
        }
    }

    match result {
        Ok(_) => session.turn_log.record_run_finished("completed"),
        Err(e) => {
            session.turn_log.record_run_finished("failed");
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// Run `--once` / positional prompt with NDJSON event stream to stdout.
///
/// Uses `execute_with_control()` to receive `ExecutorEvent`s and serializes each
/// as one JSON line. The invocation step (if host-managed) is also emitted as events.
fn run_once_json(session: &mut ail_core::session::Session, runner: &dyn Runner, prompt: &str) {
    use ail_core::executor::ExecutionControl;
    use std::collections::HashSet;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;

    let stdout = std::io::stdout();

    // Emit run_started envelope.
    {
        let mut out = stdout.lock();
        let _ = serde_json::to_writer(
            &mut out,
            &serde_json::json!({
                "type": "run_started",
                "run_id": session.run_id,
                "pipeline_source": session.pipeline.source.as_ref().map(|p| p.display().to_string()),
                "total_steps": session.pipeline.steps.len(),
            }),
        );
        let _ = writeln!(out);
        let _ = out.flush();
    }

    // Build the HITL and permission infrastructure before any step runs so that
    // both the invocation step and pipeline steps can receive permission responses
    // via the stdin control protocol.
    let (hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let pause_requested = Arc::new(AtomicBool::new(false));
    let kill_requested = Arc::new(AtomicBool::new(false));

    // One-shot channel for permission responses. When the PermissionResponder
    // callback fires, it parks a (display_name, SyncSender) here; the stdin
    // reader picks it up and optionally adds the name to the session allowlist.
    type PendingPerm = Arc<
        std::sync::Mutex<
            Option<(
                String,
                mpsc::SyncSender<ail_core::runner::PermissionResponse>,
            )>,
        >,
    >;
    let pending_permission: PendingPerm = Arc::new(std::sync::Mutex::new(None));

    // Session allowlist — tool display_names approved for the lifetime of this run.
    // Shared between the responder, event forwarding threads, and stdin reader.
    let session_allowlist: Arc<std::sync::Mutex<HashSet<String>>> =
        Arc::new(std::sync::Mutex::new(HashSet::new()));

    // Build the permission responder — checks session allowlist first, then blocks
    // until the stdin reader delivers a decision.
    let pending_perm_responder = Arc::clone(&pending_permission);
    let allowlist_responder = Arc::clone(&session_allowlist);
    let responder: ail_core::runner::PermissionResponder =
        Arc::new(move |req: ail_core::runner::PermissionRequest| {
            // Auto-approve tools already in the session allowlist — silent, no stdin block.
            if let Ok(guard) = allowlist_responder.lock() {
                if guard.contains(&req.display_name) {
                    return ail_core::runner::PermissionResponse::Allow;
                }
            }
            let (tx, rx) = mpsc::sync_channel(1);
            if let Ok(mut guard) = pending_perm_responder.lock() {
                *guard = Some((req.display_name, tx));
            }
            rx.recv_timeout(std::time::Duration::from_secs(300))
                .unwrap_or(ail_core::runner::PermissionResponse::Deny(
                    "timeout".to_string(),
                ))
        });

    // Spawn the stdin reader thread — routes NDJSON control messages from the
    // extension (or any consumer) into the executor's control channels.
    let hitl_tx_stdin = hitl_tx;
    let pause_stdin = Arc::clone(&pause_requested);
    let kill_stdin = Arc::clone(&kill_requested);
    let pending_perm_stdin = Arc::clone(&pending_permission);
    let allowlist_stdin = Arc::clone(&session_allowlist);
    std::thread::spawn(move || {
        use ail_core::protocol::{parse_control_message, ControlMessage};
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            match parse_control_message(&line) {
                Some(ControlMessage::HitlResponse(text)) => {
                    let _ = hitl_tx_stdin.send(text);
                }
                Some(ControlMessage::PermissionResponse {
                    response,
                    allow_for_session,
                }) => {
                    let mut guard = pending_perm_stdin.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some((display_name, tx)) = guard.take() {
                        if allow_for_session
                            && response == ail_core::runner::PermissionResponse::Allow
                        {
                            if let Ok(mut al) = allowlist_stdin.lock() {
                                al.insert(display_name);
                            }
                        }
                        let _ = tx.send(response);
                    }
                }
                Some(ControlMessage::Pause) => {
                    pause_stdin.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Some(ControlMessage::Resume) => {
                    pause_stdin.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                Some(ControlMessage::Kill) => {
                    kill_stdin.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                _ => {}
            }
        }
    });

    let has_invocation_step = session.has_invocation_step();

    // If the pipeline does not declare an invocation step, the host runs it first.
    if !has_invocation_step {
        // Emit step_started for invocation.
        {
            let mut out = stdout.lock();
            let _ = serde_json::to_writer(
                &mut out,
                &serde_json::json!({
                    "type": "step_started",
                    "step_id": "invocation",
                    "step_index": 0,
                    "total_steps": session.pipeline.steps.len() + 1,
                }),
            );
            let _ = writeln!(out);
            let _ = out.flush();
        }

        let invocation_options = InvokeOptions {
            model: session.cli_provider.model.clone(),
            extensions: runner.build_extensions(&session.cli_provider),
            permission_responder: Some(Arc::clone(&responder)),
            ..InvokeOptions::default()
        };

        // Spawn a thread to forward runner events as NDJSON to stdout.
        let (runner_tx, runner_rx) = mpsc::channel::<ail_core::runner::RunnerEvent>();
        let stdout_clone = std::io::stdout();
        let allowlist_fwd = Arc::clone(&session_allowlist);
        let fwd_handle = std::thread::spawn(move || {
            for ev in runner_rx {
                // Suppress permission events for session-allowlisted tools — they are
                // auto-approved by the responder and should not appear in the UI.
                if let ail_core::runner::RunnerEvent::PermissionRequested(ref req) = ev {
                    if let Ok(guard) = allowlist_fwd.lock() {
                        if guard.contains(&req.display_name) {
                            continue;
                        }
                    }
                }
                let wrapper = serde_json::json!({
                    "type": "runner_event",
                    "event": ev,
                });
                let mut out = stdout_clone.lock();
                let _ = serde_json::to_writer(&mut out, &wrapper);
                let _ = writeln!(out);
                let _ = out.flush();
            }
        });

        match runner.invoke_streaming(prompt, invocation_options, runner_tx) {
            Ok(result) => {
                let _ = fwd_handle.join();
                {
                    let mut out = stdout.lock();
                    let _ = serde_json::to_writer(
                        &mut out,
                        &serde_json::json!({
                            "type": "step_completed",
                            "step_id": "invocation",
                            "cost_usd": result.cost_usd,
                            "input_tokens": result.input_tokens,
                            "output_tokens": result.output_tokens,
                            "response": result.response,
                        }),
                    );
                    let _ = writeln!(out);
                    let _ = out.flush();
                }
                session
                    .turn_log
                    .append(ail_core::session::TurnEntry::from_prompt(
                        "invocation",
                        prompt.to_string(),
                        result,
                    ));
            }
            Err(e) => {
                let _ = fwd_handle.join();
                let mut out = stdout.lock();
                let _ = serde_json::to_writer(
                    &mut out,
                    &serde_json::json!({
                        "type": "pipeline_error",
                        "error": e.detail,
                        "error_type": e.error_type,
                    }),
                );
                let _ = writeln!(out);
                std::process::exit(1);
            }
        }
    }

    let control = ExecutionControl {
        pause_requested: Arc::clone(&pause_requested),
        kill_requested: Arc::clone(&kill_requested),
        permission_responder: Some(responder),
    };
    let disabled_steps = HashSet::new();

    // Execute pipeline steps with event streaming.
    let (event_tx, event_rx) = mpsc::channel();

    // Spawn event writer thread — serializes ExecutorEvents as NDJSON.
    let allowlist_writer = Arc::clone(&session_allowlist);
    let writer_handle = std::thread::spawn(move || {
        let stdout = std::io::stdout();
        for event in event_rx {
            // Suppress permission events for session-allowlisted tools.
            if let ail_core::executor::ExecutorEvent::RunnerEvent {
                event: ail_core::runner::RunnerEvent::PermissionRequested(ref req),
            } = event
            {
                if let Ok(guard) = allowlist_writer.lock() {
                    if guard.contains(&req.display_name) {
                        continue;
                    }
                }
            }
            let mut out = stdout.lock();
            let _ = serde_json::to_writer(&mut out, &event);
            let _ = writeln!(out);
            let _ = out.flush();
        }
    });

    let result = ail_core::executor::execute_with_control(
        session,
        runner,
        &control,
        &disabled_steps,
        event_tx,
        hitl_rx,
    );

    // Wait for writer thread to drain.
    let _ = writer_handle.join();

    match result {
        Ok(_) => {
            // PipelineCompleted event already emitted by execute_with_control.
            session.turn_log.record_run_finished("completed");
        }
        Err(e) => {
            session.turn_log.record_run_finished("failed");
            let mut out = stdout.lock();
            let _ = serde_json::to_writer(
                &mut out,
                &serde_json::json!({
                    "type": "pipeline_error",
                    "error": e.detail,
                    "error_type": e.error_type,
                }),
            );
            let _ = writeln!(out);
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    init_tracing();

    tracing::info!(event = "startup", version = ail_core::version());

    // Effective prompt: positional wins, --once as long-form alias.
    let effective_prompt = cli.prompt.clone().or(cli.once.clone());

    match (effective_prompt, cli.command) {
        (Some(prompt), None) => {
            tracing::info!(event = "once", headless = cli.headless);

            let pipeline = load_pipeline(cli.pipeline);

            let mut session = ail_core::session::Session::new(pipeline, prompt.clone());
            session.cli_provider = ail_core::config::domain::ProviderConfig {
                model: cli.model.clone(),
                base_url: cli.provider_url.clone(),
                auth_token: cli.provider_token.clone(),
                input_cost_per_1k: None,
                output_cost_per_1k: None,
            };
            let runner = match RunnerFactory::build_default(cli.headless) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            match cli.output_format {
                OutputFormat::Text => run_once_text(
                    &mut session,
                    runner.as_ref(),
                    &prompt,
                    cli.show_thinking,
                    cli.watch,
                    cli.show_work,
                ),
                OutputFormat::Json => run_once_json(&mut session, runner.as_ref(), &prompt),
            }
        }
        (None, Some(cmd)) => match cmd {
            Commands::Delete {
                run_id,
                force,
                json,
            } => {
                if let Err(e) = delete::handle_delete(run_id, force, json) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            Commands::Logs {
                session,
                query,
                format,
                tail,
                limit,
            } => {
                logs::run_logs_command(session, query, format, tail, limit);
            }
            Commands::Log {
                run_id,
                format,
                follow,
            } => {
                log::run_log_command(run_id, &format, follow);
            }
            Commands::AskUserHook { socket } => {
                ask_user_hook::run(&socket);
            }
            Commands::CheckPermissionHook { socket } => {
                check_permission_hook::run(&socket);
            }
            Commands::Materialize { pipeline, out } => {
                let p = load_pipeline(pipeline);
                let output = ail_core::materialize::materialize(&p);
                match out {
                    Some(out_path) => {
                        if let Err(e) = std::fs::write(&out_path, &output) {
                            eprintln!("Failed to write to {}: {e}", out_path.display());
                            std::process::exit(1);
                        }
                    }
                    None => print!("{output}"),
                }
            }
            Commands::Validate {
                pipeline,
                output_format,
            } => {
                let path = match ail_core::config::discovery::discover(pipeline) {
                    Some(p) => p,
                    None => {
                        match output_format {
                            OutputFormat::Json => {
                                println!(
                                    "{}",
                                    serde_json::json!({
                                        "valid": false,
                                        "errors": [{"message": "No pipeline file found.", "error_type": "ail:config/file-not-found"}]
                                    })
                                );
                            }
                            OutputFormat::Text => {
                                eprintln!("No pipeline file found.");
                            }
                        }
                        std::process::exit(1);
                    }
                };
                match ail_core::config::load(&path) {
                    Ok(p) => match output_format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({"valid": true, "step_count": p.steps.len()})
                            );
                        }
                        OutputFormat::Text => {
                            println!("Pipeline valid: {} step(s)", p.steps.len());
                        }
                    },
                    Err(e) => match output_format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "valid": false,
                                    "errors": [{"message": e.detail, "error_type": e.error_type}]
                                })
                            );
                            std::process::exit(1);
                        }
                        OutputFormat::Text => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    },
                }
            }
            Commands::Chat {
                message,
                stream,
                pipeline,
                model,
                provider_url,
                provider_token,
            } => {
                tracing::info!(
                    event = "chat",
                    one_shot = message.is_some(),
                    stream = stream
                );
                let discovered_pipeline = load_pipeline(cli.pipeline.or(pipeline));
                let cli_provider = ail_core::config::domain::ProviderConfig {
                    model,
                    base_url: provider_url,
                    auth_token: provider_token,
                    input_cost_per_1k: None,
                    output_cost_per_1k: None,
                };
                let runner = match RunnerFactory::build_default(true) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                };
                let result = if stream {
                    chat::run_chat_stream(
                        discovered_pipeline,
                        cli_provider,
                        runner.as_ref(),
                        message,
                    )
                } else {
                    chat::run_chat_text(discovered_pipeline, cli_provider, runner.as_ref(), message)
                };
                if let Err(e) = result {
                    eprintln!("chat error: {e}");
                    std::process::exit(1);
                }
            }
        },
        (None, None) => {
            // No prompt and no subcommand — print usage hint and exit.
            eprintln!("Usage: ail <PROMPT> [OPTIONS]");
            eprintln!("       ail --once <PROMPT> [OPTIONS]");
            eprintln!("       ail <SUBCOMMAND> [OPTIONS]");
            eprintln!();
            eprintln!("Run `ail --help` for full usage.");
            std::process::exit(0);
        }
        (Some(_), Some(_)) => {
            // clap prevents this via conflicts_with, but handle defensively.
            unreachable!("clap should have rejected prompt + subcommand combination");
        }
    }
}
