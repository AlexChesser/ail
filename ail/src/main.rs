mod cli;
mod mcp_bridge;
mod tui;

use ail_core::runner::claude::ClaudeInvokeExtensions;
use ail_core::runner::factory::RunnerFactory;
use ail_core::runner::{InvokeOptions, Runner};
use clap::Parser;
use cli::{Cli, Commands, OutputFormat};

/// Initialise tracing. In TUI mode, write to a log file so output doesn't corrupt the
/// alternate screen. In all other modes, write to stderr.
fn init_tracing(tui_mode: bool) {
    if tui_mode {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".ail");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("tui.log"))
            .expect("failed to open ~/.ail/tui.log");
        tracing_subscriber::fmt()
            .json()
            .with_writer(std::sync::Mutex::new(log_file))
            .init();
    } else {
        tracing_subscriber::fmt()
            .json()
            .with_writer(std::io::stderr)
            .init();
    }
}

/// Run `--once` with human-readable text output (default behaviour).
///
/// When `show_thinking` or `show_responses` is set, uses streaming execution to print
/// per-step progress, thinking blocks, and/or responses as they arrive.
fn run_once_text(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    prompt: &str,
    show_thinking: bool,
    show_responses: bool,
) {
    let has_invocation_step = session
        .pipeline
        .steps
        .first()
        .map(|s| s.id.as_str() == "invocation")
        .unwrap_or(false);

    if !has_invocation_step {
        let invocation_options = InvokeOptions {
            model: session.cli_provider.model.clone(),
            extensions: Some(Box::new(ClaudeInvokeExtensions {
                base_url: session.cli_provider.base_url.clone(),
                auth_token: session.cli_provider.auth_token.clone(),
                permission_socket: None,
            })),
            ..InvokeOptions::default()
        };
        match runner.invoke(prompt, invocation_options) {
            Ok(result) => {
                if show_responses {
                    println!(
                        "[1/{}] invocation ({} in / {} out)",
                        session.pipeline.steps.len() + 1,
                        result.input_tokens,
                        result.output_tokens
                    );
                    println!("\n  [Response]\n{}\n", result.response);
                }
                session.turn_log.append(ail_core::session::TurnEntry {
                    step_id: "invocation".to_string(),
                    prompt: prompt.to_string(),
                    response: Some(result.response),
                    timestamp: std::time::SystemTime::now(),
                    cost_usd: result.cost_usd,
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
                    runner_session_id: result.session_id,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                });
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }

    if show_thinking || show_responses {
        run_once_text_verbose(session, runner, show_thinking, show_responses);
    } else {
        run_once_text_quiet(session, runner, has_invocation_step);
    }
}

/// Quiet path: no per-step output, just print the final response(s).
fn run_once_text_quiet(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    has_invocation_step: bool,
) {
    match ail_core::executor::execute(session, runner) {
        Ok(outcome) => {
            use ail_core::executor::ExecuteOutcome;
            if let ExecuteOutcome::Break { step_id } = outcome {
                tracing::info!(event = "pipeline_break", step_id = %step_id);
            }
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
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// Verbose path: print per-step progress + optional thinking/response blocks.
///
/// Uses `execute_with_control` so `RunnerEvent::Thinking` and `RunnerEvent::StreamDelta`
/// events are available for display. The unbounded mpsc channel means execute_with_control
/// never blocks; events are drained after it returns.
fn run_once_text_verbose(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    show_thinking: bool,
    show_responses: bool,
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
                if show_responses && !response_buf.is_empty() {
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

    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// Run `--once` with NDJSON event stream to stdout.
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
    }

    let has_invocation_step = session
        .pipeline
        .steps
        .first()
        .map(|s| s.id.as_str() == "invocation")
        .unwrap_or(false);

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
        }

        let invocation_options = InvokeOptions {
            model: session.cli_provider.model.clone(),
            extensions: Some(Box::new(ClaudeInvokeExtensions {
                base_url: session.cli_provider.base_url.clone(),
                auth_token: session.cli_provider.auth_token.clone(),
                permission_socket: None,
            })),
            ..InvokeOptions::default()
        };

        // Spawn a thread to forward runner events as NDJSON to stdout.
        let (runner_tx, runner_rx) = mpsc::channel::<ail_core::runner::RunnerEvent>();
        let stdout_clone = std::io::stdout();
        let fwd_handle = std::thread::spawn(move || {
            for ev in runner_rx {
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
                        }),
                    );
                    let _ = writeln!(out);
                }
                session.turn_log.append(ail_core::session::TurnEntry {
                    step_id: "invocation".to_string(),
                    prompt: prompt.to_string(),
                    response: Some(result.response),
                    timestamp: std::time::SystemTime::now(),
                    cost_usd: result.cost_usd,
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
                    runner_session_id: result.session_id,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                });
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

    // Execute pipeline steps with event streaming.
    let (event_tx, event_rx) = mpsc::channel();
    let (hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let pause_requested = Arc::new(AtomicBool::new(false));
    let kill_requested = Arc::new(AtomicBool::new(false));

    // One-shot channel for permission responses. When the PermissionResponder
    // callback fires, it parks a SyncSender here; the stdin reader picks it up.
    let pending_permission: Arc<
        std::sync::Mutex<Option<mpsc::SyncSender<ail_core::runner::PermissionResponse>>>,
    > = Arc::new(std::sync::Mutex::new(None));

    // Build the permission responder — blocks until the stdin reader delivers a decision.
    let pending_perm_responder = Arc::clone(&pending_permission);
    let responder: ail_core::runner::PermissionResponder =
        Arc::new(move |_req: ail_core::runner::PermissionRequest| {
            let (tx, rx) = mpsc::sync_channel(1);
            if let Ok(mut guard) = pending_perm_responder.lock() {
                *guard = Some(tx);
            }
            rx.recv_timeout(std::time::Duration::from_secs(300))
                .unwrap_or(ail_core::runner::PermissionResponse::Deny(
                    "timeout".to_string(),
                ))
        });

    let control = ExecutionControl {
        pause_requested: Arc::clone(&pause_requested),
        kill_requested: Arc::clone(&kill_requested),
        permission_responder: Some(responder),
    };
    let disabled_steps = HashSet::new();

    // Spawn the stdin reader thread — routes NDJSON control messages from the
    // extension (or any consumer) into the executor's control channels.
    let hitl_tx_stdin = hitl_tx;
    let pause_stdin = Arc::clone(&pause_requested);
    let kill_stdin = Arc::clone(&kill_requested);
    let pending_perm_stdin = Arc::clone(&pending_permission);
    std::thread::spawn(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) if !l.is_empty() => l,
                Ok(_) => continue,
                Err(_) => break,
            };
            let msg: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match msg.get("type").and_then(|t| t.as_str()) {
                Some("hitl_response") => {
                    let text = msg
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = hitl_tx_stdin.send(text);
                }
                Some("permission_response") => {
                    let allowed = msg
                        .get("allowed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let response = if allowed {
                        ail_core::runner::PermissionResponse::Allow
                    } else {
                        let reason = msg
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        ail_core::runner::PermissionResponse::Deny(reason)
                    };
                    let mut guard = pending_perm_stdin.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(response);
                    }
                }
                Some("pause") => {
                    pause_stdin.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                Some("resume") => {
                    pause_stdin.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                Some("kill") => {
                    kill_stdin.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                _ => {}
            }
        }
    });

    // Spawn event writer thread — serializes ExecutorEvents as NDJSON.
    let writer_handle = std::thread::spawn(move || {
        let stdout = std::io::stdout();
        for event in event_rx {
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
        }
        Err(e) => {
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

    // Determine if we're launching the TUI (no subcommand, no --once).
    let tui_mode = cli.command.is_none() && cli.once.is_none();
    init_tracing(tui_mode);

    tracing::info!(event = "startup", version = ail_core::version());

    match cli.command {
        Some(Commands::McpBridge { socket }) => {
            // Spawned by Claude CLI to handle tool permission checks.
            // Does not initialise tracing — only stdout must be used for MCP protocol.
            mcp_bridge::run(&socket);
        }
        Some(Commands::Materialize { pipeline, out }) => {
            let pipeline_path = ail_core::config::discovery::discover(pipeline);
            let p = match pipeline_path {
                Some(ref path) => match ail_core::config::load(path) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                },
                None => ail_core::config::domain::Pipeline::passthrough(),
            };
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
        Some(Commands::Validate {
            pipeline,
            output_format,
        }) => {
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
        None => {
            if let Some(prompt) = cli.once {
                tracing::info!(event = "once", headless = cli.headless);

                let pipeline_path = ail_core::config::discovery::discover(cli.pipeline);
                let pipeline = match pipeline_path {
                    Some(ref path) => match ail_core::config::load(path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    },
                    None => ail_core::config::domain::Pipeline::passthrough(),
                };

                let mut session = ail_core::session::Session::new(pipeline, prompt.clone());
                session.cli_provider = ail_core::config::domain::ProviderConfig {
                    model: cli.model.clone(),
                    base_url: cli.provider_url.clone(),
                    auth_token: cli.provider_token.clone(),
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
                        cli.show_responses,
                    ),
                    OutputFormat::Json => run_once_json(&mut session, runner.as_ref(), &prompt),
                }
            } else {
                tracing::info!(event = "tui_launch");
                let pipeline_path = ail_core::config::discovery::discover(cli.pipeline);
                let pipeline = match pipeline_path {
                    Some(ref path) => match ail_core::config::load(path) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    },
                    None => None,
                };
                let cli_provider = ail_core::config::domain::ProviderConfig {
                    model: cli.model.clone(),
                    base_url: cli.provider_url.clone(),
                    auth_token: cli.provider_token.clone(),
                };
                let runner = match RunnerFactory::build_default(cli.headless) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                };
                if let Err(e) = tui::run(pipeline, cli_provider, runner) {
                    eprintln!("TUI error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
