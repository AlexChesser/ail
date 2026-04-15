//! Interactive chat mode — multi-turn conversation with pipeline execution after each message.
//!
//! # NDJSON protocol (stdout)
//!
//! | Event              | Fields                                              |
//! |--------------------|-----------------------------------------------------|
//! | `chat_started`     | `chat_session_id`, `pipeline_source`                |
//! | `turn_started`     | `turn_id`, `prompt`                                 |
//! | `run_started`      | per-turn, same as `--once`                          |
//! | `step_*`, `runner_event`, `pipeline_completed` | per-turn events          |
//! | `turn_completed`   | `turn_id`, `total_cost_usd`, `duration_ms`          |
//! | `ready`            | (empty — signals consumer may send next message)    |
//! | `chat_ended`       | `chat_session_id`, `total_turns`                    |
//!
//! # NDJSON protocol (stdin)
//!
//! | Type                | Purpose                              |
//! |---------------------|--------------------------------------|
//! | `user_message`      | New prompt (`text` field)            |
//! | `hitl_response`     | HITL gate response (`text` field)    |
//! | `permission_response` | Tool permission (`allowed`, `reason`)|
//! | `pause`             | Pause current step                   |
//! | `resume`            | Resume paused step                   |
//! | `kill`              | Kill current step                    |
//! | `end_session`       | Graceful close                       |
//!
//! Bare non-JSON lines on stdin are treated as `user_message` for terminal ergonomics.

use ail_core::config::domain::{Pipeline, ProviderConfig};
use ail_core::executor::{ExecutionControl, ExecutorEvent};
use ail_core::runner::{CancelToken, InvokeOptions, Runner, RunnerEvent};
use ail_core::session::{Session, TurnEntry};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::control_bridge;

/// Emit a single NDJSON line to stdout (locked).
fn emit(value: &serde_json::Value) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = serde_json::to_writer(&mut out, value);
    let _ = writeln!(out);
    let _ = out.flush();
}

/// Run one turn of the chat loop (invocation + pipeline steps), streaming events to stdout.
///
/// Returns `(last_runner_session_id, turn_cost_usd)` on success,
/// or `Err(detail)` on fatal invocation failure (error already emitted as `pipeline_error`).
#[allow(clippy::too_many_arguments)]
async fn run_turn_stream(
    pipeline: Pipeline,
    cli_provider: &ProviderConfig,
    runner: &(dyn Runner + Sync),
    prompt: &str,
    resume_session_id: Option<&str>,
    turn_id: &str,
    pause_requested: Arc<AtomicBool>,
    kill_requested: CancelToken,
    pending_permission: control_bridge::PendingPermSlot,
    session_allowlist: control_bridge::AllowlistArc,
    hitl_rx: mpsc::Receiver<String>,
) -> Result<(Option<String>, f64), String> {
    let mut session = Session::new(pipeline, prompt.to_string());
    session.cli_provider = cli_provider.clone();

    emit(&serde_json::json!({
        "type": "run_started",
        "run_id": session.run_id,
        "turn_id": turn_id,
        "pipeline_source": session.pipeline.source.as_ref().map(|p| p.display().to_string()),
        "total_steps": session.pipeline.steps.len(),
    }));

    let has_invocation_step = session.has_invocation_step();

    // Build the permission responder from the shared helper. It auto-approves
    // tools already in `session_allowlist`; for unknown tools it parks a
    // SyncSender (tagged with the request's `display_name`) in
    // `pending_permission` where the stdin reader finds it via
    // `apply_permission_response`. Same one-slot design used by
    // `--once --output-format json` mode.
    let responder = control_bridge::make_allowlist_responder(
        Arc::clone(&pending_permission),
        Arc::clone(&session_allowlist),
    );

    // Host-managed invocation step.
    if !has_invocation_step {
        emit(&serde_json::json!({
            "type": "step_started",
            "step_id": "invocation",
            "step_index": 0,
            "total_steps": session.pipeline.steps.len() + 1,
        }));

        let invocation_options = InvokeOptions {
            resume_session_id: resume_session_id.map(|s| s.to_string()),
            model: session.cli_provider.model.clone(),
            extensions: runner.build_extensions(&session.cli_provider),
            permission_responder: Some(Arc::clone(&responder)),
            ..InvokeOptions::default()
        };

        // Spawn blocking task to forward runner events as NDJSON. Allowlisted
        // tools have their `PermissionRequested` events suppressed so the
        // consumer never sees a request for a pre-approved tool (SPEC §13.2
        // "auto-approved silently").
        let (runner_tx, runner_rx) = mpsc::channel::<RunnerEvent>();
        let fwd_handle =
            control_bridge::spawn_runner_event_writer(runner_rx, Arc::clone(&session_allowlist));

        let invoke_result = tokio::task::block_in_place(|| {
            runner.invoke_streaming(prompt, invocation_options, runner_tx)
        });
        let _ = fwd_handle.await;

        match invoke_result {
            Ok(result) => {
                emit(&serde_json::json!({
                    "type": "step_completed",
                    "step_id": "invocation",
                    "cost_usd": result.cost_usd,
                    "input_tokens": result.input_tokens,
                    "output_tokens": result.output_tokens,
                }));
                session.turn_log.append(TurnEntry::from_prompt(
                    "invocation",
                    prompt.to_string(),
                    result,
                ));
            }
            Err(e) => {
                emit(&serde_json::json!({
                    "type": "pipeline_error",
                    "error": e.detail(),
                    "error_type": e.error_type(),
                }));
                return Err(e.into_detail());
            }
        }
    }

    let control = ExecutionControl {
        pause_requested: Arc::clone(&pause_requested),
        kill_requested: kill_requested.clone(),
        permission_responder: Some(responder),
    };
    let disabled_steps = HashSet::new();
    let (event_tx, event_rx) = mpsc::channel::<ExecutorEvent>();

    // Drain executor events to stdout; suppress PermissionRequested events
    // for tools already in the session allowlist.
    let writer_handle =
        control_bridge::spawn_executor_event_writer(event_rx, Some(Arc::clone(&session_allowlist)));

    let exec_result = tokio::task::block_in_place(|| {
        ail_core::executor::execute_with_control(
            &mut session,
            runner,
            &control,
            &disabled_steps,
            event_tx,
            hitl_rx,
        )
    });

    let _ = writer_handle.await;

    let turn_cost = session
        .turn_log
        .entries()
        .iter()
        .filter_map(|e| e.cost_usd)
        .sum::<f64>();

    let last_runner_session_id = session
        .turn_log
        .last_runner_session_id()
        .map(|s| s.to_string());

    match exec_result {
        Ok(_) => Ok((last_runner_session_id, turn_cost)),
        Err(e) => {
            emit(&serde_json::json!({
                "type": "pipeline_error",
                "error": e.detail(),
                "error_type": e.error_type(),
            }));
            Err(e.into_detail())
        }
    }
}

/// Run the chat loop in NDJSON stream mode.
///
/// If `initial_message` is `Some`, that message is processed first; in one-shot mode
/// (`--message`) the loop exits after that single turn without reading stdin.
pub async fn run_chat_stream(
    pipeline: Pipeline,
    cli_provider: ProviderConfig,
    runner: &(dyn Runner + Sync),
    initial_message: Option<String>,
) -> Result<(), String> {
    let chat_session_id = Uuid::new_v4().to_string();
    let one_shot = initial_message.is_some();

    emit(&serde_json::json!({
        "type": "chat_started",
        "chat_session_id": chat_session_id,
        "pipeline_source": pipeline.source.as_ref().map(|p| p.display().to_string()),
        "one_shot": one_shot,
    }));

    // Shared control flags reused across turns.
    let pause_requested = Arc::new(AtomicBool::new(false));
    let kill_requested = CancelToken::new();

    // Shared permission/allowlist state. Scoped to the whole chat session —
    // the allowlist persists across turns so "Allow for session" survives
    // past the turn in which it was granted. Same shape used by `--once
    // --output-format json` mode.
    let pending_permission = control_bridge::make_pending_perm();
    let session_allowlist = control_bridge::make_allowlist();

    // Async channel for user prompts (None = shutdown sentinel).
    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::channel::<Option<String>>(32);

    // Slot so the stdin reader can target HITL responses to the current turn.
    let hitl_slot: Arc<std::sync::Mutex<Option<mpsc::SyncSender<String>>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Enqueue the initial message if provided (one-shot mode).
    if let Some(msg) = initial_message {
        let _ = prompt_tx.send(Some(msg)).await;
        // In one-shot mode we immediately send the shutdown sentinel after the message.
        let _ = prompt_tx.send(None).await;
    }

    control_bridge::spawn_stdin_reader_chat(
        tokio::io::BufReader::new(tokio::io::stdin()),
        prompt_tx,
        Arc::clone(&hitl_slot),
        Arc::clone(&pending_permission),
        Arc::clone(&session_allowlist),
        Arc::clone(&pause_requested),
        kill_requested.clone(),
    );

    let mut last_runner_session_id: Option<String> = None;
    let mut turn_count: usize = 0;
    let mut total_cost_usd: f64 = 0.0;

    while let Some(Some(prompt)) = prompt_rx.recv().await {
        {
            if prompt.trim().is_empty() {
                continue;
            }

            let turn_id = Uuid::new_v4().to_string();
            emit(&serde_json::json!({
                "type": "turn_started",
                "turn_id": turn_id,
                "prompt": prompt,
            }));

            // Reset cancel token between turns.
            kill_requested.reset();

            // Install per-turn HITL channel.
            let (hitl_sync_tx, hitl_sync_rx) = mpsc::sync_channel::<String>(32);
            let (hitl_tx, hitl_rx) = mpsc::channel::<String>();
            {
                let mut guard = hitl_slot.lock().unwrap_or_else(|e| e.into_inner());
                *guard = Some(hitl_sync_tx);
            }
            // Bridge the sync channel to the regular mpsc channel the executor expects.
            tokio::task::spawn_blocking(move || {
                for msg in hitl_sync_rx {
                    if hitl_tx.send(msg).is_err() {
                        break;
                    }
                }
            });

            let start = Instant::now();
            let result = run_turn_stream(
                pipeline.clone(),
                &cli_provider,
                runner,
                &prompt,
                last_runner_session_id.as_deref(),
                &turn_id,
                Arc::clone(&pause_requested),
                kill_requested.clone(),
                Arc::clone(&pending_permission),
                Arc::clone(&session_allowlist),
                hitl_rx,
            )
            .await;

            // Clear the HITL slot after the turn completes. The permission
            // slot is not cleared here — stale senders are dropped
            // automatically when `run_turn_stream` returns (the
            // `SyncSender` is held on the blocking thread stack and drops
            // on unwind), and `apply_permission_response` silently skips
            // delivery when the slot is empty.
            {
                let mut guard = hitl_slot.lock().unwrap_or_else(|e| e.into_inner());
                *guard = None;
            }

            let duration_ms = start.elapsed().as_millis();
            turn_count += 1;

            match result {
                Ok((new_session_id, turn_cost)) => {
                    if new_session_id.is_some() {
                        last_runner_session_id = new_session_id;
                    }
                    total_cost_usd += turn_cost;
                    emit(&serde_json::json!({
                        "type": "turn_completed",
                        "turn_id": turn_id,
                        "total_cost_usd": total_cost_usd,
                        "duration_ms": duration_ms,
                    }));
                }
                Err(_) => {
                    // Error already emitted as pipeline_error inside run_turn_stream.
                    emit(&serde_json::json!({
                        "type": "turn_completed",
                        "turn_id": turn_id,
                        "total_cost_usd": total_cost_usd,
                        "duration_ms": duration_ms,
                        "error": true,
                    }));
                }
            }

            emit(&serde_json::json!({ "type": "ready" }));
        }
    }

    emit(&serde_json::json!({
        "type": "chat_ended",
        "chat_session_id": chat_session_id,
        "total_turns": turn_count,
        "total_cost_usd": total_cost_usd,
    }));

    Ok(())
}

/// Run the chat loop in plain-text mode (human-readable, no NDJSON envelope).
///
/// Reads prompts from stdin line-by-line and prints each turn's final response.
pub fn run_chat_text(
    pipeline: Pipeline,
    cli_provider: ProviderConfig,
    runner: &(dyn Runner + Sync),
    initial_message: Option<String>,
) -> Result<(), String> {
    use std::io::BufRead;

    let one_shot = initial_message.is_some();
    let mut last_runner_session_id: Option<String> = None;

    let do_turn = |prompt: &str, last_id: Option<&str>| -> Result<Option<String>, String> {
        let mut session = Session::new(pipeline.clone(), prompt.to_string());
        session.cli_provider = cli_provider.clone();

        if !session.has_invocation_step() {
            let options = InvokeOptions {
                resume_session_id: last_id.map(|s| s.to_string()),
                model: session.cli_provider.model.clone(),
                extensions: runner.build_extensions(&session.cli_provider),
                ..InvokeOptions::default()
            };
            ail_core::executor::run_invocation_step(&mut session, runner, prompt, options)
                .map_err(|e| e.into_detail())?;
        }

        let pause_requested = Arc::new(AtomicBool::new(false));
        let kill_requested = CancelToken::new();
        let control = ExecutionControl {
            pause_requested,
            kill_requested,
            permission_responder: None,
        };
        let disabled_steps = HashSet::new();
        let (event_tx, _event_rx) = mpsc::channel::<ExecutorEvent>();
        let (_hitl_tx, hitl_rx) = mpsc::channel::<String>();

        match ail_core::executor::execute_with_control(
            &mut session,
            runner,
            &control,
            &disabled_steps,
            event_tx,
            hitl_rx,
        ) {
            Ok(_) => {}
            Err(e) => return Err(e.into_detail()),
        }

        // Print invocation response (if pipeline declared it) and last pipeline step response.
        if session.has_invocation_step() {
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

        Ok(session
            .turn_log
            .last_runner_session_id()
            .map(|s| s.to_string()))
    };

    if let Some(msg) = initial_message {
        match do_turn(&msg, last_runner_session_id.as_deref()) {
            Ok(new_id) => {
                if new_id.is_some() {
                    last_runner_session_id = new_id;
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        }
        if one_shot {
            return Ok(());
        }
    }

    // Interactive loop: read prompts from stdin.
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let prompt = line.trim().to_string();
        if prompt.is_empty() {
            continue;
        }
        match do_turn(&prompt, last_runner_session_id.as_deref()) {
            Ok(new_id) => {
                if new_id.is_some() {
                    last_runner_session_id = new_id;
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        }
    }

    Ok(())
}
