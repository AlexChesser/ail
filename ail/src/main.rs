mod cli;
mod mcp_bridge;
mod tui;

use ail_core::runner::claude::{ClaudeCliRunnerConfig, ClaudeInvokeExtensions};
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
fn run_once_text(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    prompt: &str,
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
                println!("{}", result.response);
                session.turn_log.append(ail_core::session::TurnEntry {
                    step_id: "invocation".to_string(),
                    prompt: prompt.to_string(),
                    response: Some(result.response),
                    timestamp: std::time::SystemTime::now(),
                    cost_usd: result.cost_usd,
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

    match ail_core::executor::execute(session, runner) {
        Ok(outcome) => {
            use ail_core::executor::ExecuteOutcome;
            match outcome {
                ExecuteOutcome::Break { step_id } => {
                    tracing::info!(event = "pipeline_break", step_id = %step_id);
                }
                ExecuteOutcome::Completed => {}
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

/// Run `--once` with NDJSON event stream to stdout.
///
/// Uses `execute_with_control()` to receive `ExecutorEvent`s and serializes each
/// as one JSON line. The invocation step (if host-managed) is also emitted as events.
fn run_once_json(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    prompt: &str,
) {
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
        let _ = serde_json::to_writer(&mut out, &serde_json::json!({
            "type": "run_started",
            "run_id": session.run_id,
            "pipeline_source": session.pipeline.source.as_ref().map(|p| p.display().to_string()),
            "total_steps": session.pipeline.steps.len(),
        }));
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
            let _ = serde_json::to_writer(&mut out, &serde_json::json!({
                "type": "step_started",
                "step_id": "invocation",
                "step_index": 0,
                "total_steps": session.pipeline.steps.len() + 1,
            }));
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
        match runner.invoke(prompt, invocation_options) {
            Ok(result) => {
                {
                    let mut out = stdout.lock();
                    let _ = serde_json::to_writer(&mut out, &serde_json::json!({
                        "type": "step_completed",
                        "step_id": "invocation",
                        "cost_usd": result.cost_usd,
                    }));
                    let _ = writeln!(out);
                }
                session.turn_log.append(ail_core::session::TurnEntry {
                    step_id: "invocation".to_string(),
                    prompt: prompt.to_string(),
                    response: Some(result.response),
                    timestamp: std::time::SystemTime::now(),
                    cost_usd: result.cost_usd,
                    runner_session_id: result.session_id,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                });
            }
            Err(e) => {
                let mut out = stdout.lock();
                let _ = serde_json::to_writer(&mut out, &serde_json::json!({
                    "type": "pipeline_error",
                    "error": e.detail,
                    "error_type": e.error_type,
                }));
                let _ = writeln!(out);
                std::process::exit(1);
            }
        }
    }

    // Execute pipeline steps with event streaming.
    let (event_tx, event_rx) = mpsc::channel();
    let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();
    let control = ExecutionControl {
        pause_requested: Arc::new(AtomicBool::new(false)),
        kill_requested: Arc::new(AtomicBool::new(false)),
        permission_responder: None,
    };
    let disabled_steps = HashSet::new();

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
            let _ = serde_json::to_writer(&mut out, &serde_json::json!({
                "type": "pipeline_error",
                "error": e.detail,
                "error_type": e.error_type,
            }));
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
        Some(Commands::Validate { pipeline }) => {
            let path = match ail_core::config::discovery::discover(pipeline) {
                Some(p) => p,
                None => {
                    eprintln!("No pipeline file found.");
                    std::process::exit(1);
                }
            };
            match ail_core::config::load(&path) {
                Ok(p) => {
                    println!("Pipeline valid: {} step(s)", p.steps.len());
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
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
                let runner = ClaudeCliRunnerConfig::default()
                    .headless(cli.headless)
                    .build();

                match cli.output_format {
                    OutputFormat::Text => run_once_text(&mut session, &runner, &prompt),
                    OutputFormat::Json => run_once_json(&mut session, &runner, &prompt),
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
                let runner = Box::new(
                    ClaudeCliRunnerConfig::default()
                        .headless(cli.headless)
                        .build(),
                );
                if let Err(e) = tui::run(pipeline, cli_provider, runner) {
                    eprintln!("TUI error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
