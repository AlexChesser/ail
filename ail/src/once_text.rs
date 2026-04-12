use ail_core::executor::ExecuteOutcome;
use ail_core::runner::{InvokeOptions, Runner};

/// Run `--once` / positional prompt with human-readable text output (default lean mode).
///
/// When `show_thinking` or `watch` is set, uses streaming execution to print
/// per-step progress, thinking blocks, and/or responses as they arrive.
pub fn run_once_text(
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
        run_once_text_show_work(session, runner, run_start);
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
    run_start: std::time::Instant,
) {
    match ail_core::executor::execute(session, runner) {
        Ok(outcome) => {
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
        kill_requested: ail_core::runner::CancelToken::new(),
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
