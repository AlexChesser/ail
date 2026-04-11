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
use ail_core::runner::{InvokeOptions, PermissionResponse, Runner, RunnerEvent};
use ail_core::session::{Session, TurnEntry};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

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
    runner: &dyn Runner,
    prompt: &str,
    resume_session_id: Option<&str>,
    turn_id: &str,
    pause_requested: Arc<AtomicBool>,
    kill_requested: Arc<AtomicBool>,
    pending_permission: Arc<
        std::sync::Mutex<Option<mpsc::SyncSender<ail_core::runner::PermissionResponse>>>,
    >,
    perm_notify: Arc<tokio::sync::Notify>,
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

    // Build per-turn permission responder. Signals perm_notify after parking the
    // sender so the bridge task (in run_chat_stream) can route stdin responses.
    let pending_perm = Arc::clone(&pending_permission);
    let perm_notify_responder = Arc::clone(&perm_notify);
    let responder: ail_core::runner::PermissionResponder =
        Arc::new(move |_req: ail_core::runner::PermissionRequest| {
            let (tx, rx) = mpsc::sync_channel(1);
            if let Ok(mut guard) = pending_perm.lock() {
                *guard = Some(tx);
            }
            perm_notify_responder.notify_one();
            rx.recv_timeout(std::time::Duration::from_secs(300))
                .unwrap_or(PermissionResponse::Deny("timeout".to_string()))
        });

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

        // Spawn blocking task to forward runner events as NDJSON.
        let (runner_tx, runner_rx) = mpsc::channel::<RunnerEvent>();
        let fwd_handle = tokio::task::spawn_blocking(move || {
            for ev in runner_rx {
                emit(&serde_json::json!({
                    "type": "runner_event",
                    "event": ev,
                }));
            }
        });

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
        kill_requested: Arc::clone(&kill_requested),
        permission_responder: Some(responder),
    };
    let disabled_steps = HashSet::new();
    let (event_tx, event_rx) = mpsc::channel::<ExecutorEvent>();

    // Spawn blocking task to drain executor events to stdout.
    let writer_handle = tokio::task::spawn_blocking(move || {
        for event in event_rx {
            if let Ok(v) = serde_json::to_value(&event) {
                emit(&v);
            }
        }
    });

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
    runner: &dyn Runner,
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
    let kill_requested = Arc::new(AtomicBool::new(false));
    let pending_permission: Arc<
        std::sync::Mutex<Option<mpsc::SyncSender<ail_core::runner::PermissionResponse>>>,
    > = Arc::new(std::sync::Mutex::new(None));

    // Async channel for user prompts (None = shutdown sentinel).
    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::channel::<Option<String>>(32);

    // Slots so the stdin reader can target control messages to the current turn.
    let hitl_slot: Arc<std::sync::Mutex<Option<mpsc::SyncSender<String>>>> =
        Arc::new(std::sync::Mutex::new(None));
    let perm_slot: Arc<std::sync::Mutex<Option<mpsc::SyncSender<PermissionResponse>>>> =
        Arc::new(std::sync::Mutex::new(None));

    // Enqueue the initial message if provided (one-shot mode).
    if let Some(msg) = initial_message {
        let _ = prompt_tx.send(Some(msg)).await;
        // In one-shot mode we immediately send the shutdown sentinel after the message.
        let _ = prompt_tx.send(None).await;
    }

    // Async stdin reader — always running; in one-shot mode it only reads control messages.
    {
        let prompt_tx_stdin = prompt_tx;
        let pause_stdin = Arc::clone(&pause_requested);
        let kill_stdin = Arc::clone(&kill_requested);
        let hitl_slot_stdin = Arc::clone(&hitl_slot);
        let perm_slot_stdin = Arc::clone(&perm_slot);

        tokio::spawn(async move {
            use ail_core::protocol::{parse_control_message, ControlMessage};
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match parse_control_message(&line) {
                    Some(ControlMessage::UserMessage(text)) => {
                        if prompt_tx_stdin.send(Some(text)).await.is_err() {
                            break;
                        }
                    }
                    Some(ControlMessage::EndSession) => {
                        let _ = prompt_tx_stdin.send(None).await;
                        break;
                    }
                    Some(ControlMessage::HitlResponse(text)) => {
                        let guard = hitl_slot_stdin.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(ref tx) = *guard {
                            let _ = tx.send(text);
                        }
                    }
                    Some(ControlMessage::PermissionResponse { response, .. }) => {
                        let guard = perm_slot_stdin.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(ref tx) = *guard {
                            let _ = tx.send(response);
                        }
                    }
                    Some(ControlMessage::Pause) => {
                        pause_stdin.store(true, Ordering::SeqCst);
                    }
                    Some(ControlMessage::Resume) => {
                        pause_stdin.store(false, Ordering::SeqCst);
                    }
                    Some(ControlMessage::Kill) => {
                        kill_stdin.store(true, Ordering::SeqCst);
                    }
                    None => {}
                }
            }
            // EOF — signal shutdown.
            let _ = prompt_tx_stdin.send(None).await;
        });
    }

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

            // Reset kill flag between turns.
            kill_requested.store(false, Ordering::SeqCst);

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

            // Permission bridge: replaces the former polling sleep loop.
            // The per-turn responder (in run_turn_stream) signals perm_notify after
            // parking the sender in pending_permission. This task wakes immediately,
            // moves the sender to perm_slot so the stdin reader can deliver a response.
            let perm_notify = Arc::new(tokio::sync::Notify::new());
            {
                let pending_perm_bridge = Arc::clone(&pending_permission);
                let perm_slot_bridge = Arc::clone(&perm_slot);
                let perm_notify_bridge = Arc::clone(&perm_notify);
                tokio::spawn(async move {
                    perm_notify_bridge.notified().await;
                    let tx_opt = pending_perm_bridge
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .take();
                    if let Some(tx) = tx_opt {
                        let mut guard = perm_slot_bridge.lock().unwrap_or_else(|e| e.into_inner());
                        *guard = Some(tx);
                    }
                });
            }

            let start = Instant::now();
            let result = run_turn_stream(
                pipeline.clone(),
                &cli_provider,
                runner,
                &prompt,
                last_runner_session_id.as_deref(),
                &turn_id,
                Arc::clone(&pause_requested),
                Arc::clone(&kill_requested),
                Arc::clone(&pending_permission),
                perm_notify,
                hitl_rx,
            )
            .await;

            // Clear slots after turn completes.
            {
                let mut guard = hitl_slot.lock().unwrap_or_else(|e| e.into_inner());
                *guard = None;
            }
            {
                let mut guard = perm_slot.lock().unwrap_or_else(|e| e.into_inner());
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
    runner: &dyn Runner,
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
        let kill_requested = Arc::new(AtomicBool::new(false));
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
