use ail_core::runner::{InvokeOptions, Runner};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

/// Run `--once` / positional prompt with NDJSON event stream to stdout.
///
/// Uses `execute_with_control()` to receive `ExecutorEvent`s and serializes each
/// as one JSON line. The invocation step (if host-managed) is also emitted as events.
pub async fn run_once_json(
    session: &mut ail_core::session::Session,
    runner: &dyn Runner,
    prompt: &str,
) {
    use ail_core::executor::ExecutionControl;

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
    // Shared between the responder, event forwarding tasks, and stdin reader.
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

    // Async stdin reader — routes NDJSON control messages from the extension (or any
    // consumer) into the executor's control channels.
    let hitl_tx_stdin = hitl_tx;
    let pause_stdin = Arc::clone(&pause_requested);
    let kill_stdin = Arc::clone(&kill_requested);
    let pending_perm_stdin = Arc::clone(&pending_permission);
    let allowlist_stdin = Arc::clone(&session_allowlist);
    tokio::spawn(async move {
        use ail_core::protocol::{parse_control_message, ControlMessage};
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
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

        // Spawn a blocking task to forward runner events as NDJSON to stdout.
        let (runner_tx, runner_rx) = mpsc::channel::<ail_core::runner::RunnerEvent>();
        let stdout_clone = std::io::stdout();
        let allowlist_fwd = Arc::clone(&session_allowlist);
        let fwd_handle = tokio::task::spawn_blocking(move || {
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

        // Run invoke_streaming on the current thread (block_in_place keeps other
        // tokio tasks running on other threads while this one blocks).
        let invoke_result = tokio::task::block_in_place(|| {
            runner.invoke_streaming(prompt, invocation_options, runner_tx)
        });
        // runner_tx consumed by invoke_streaming; fwd task drains remaining events.
        let _ = fwd_handle.await;

        match invoke_result {
            Ok(result) => {
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
                let mut out = stdout.lock();
                let _ = serde_json::to_writer(
                    &mut out,
                    &serde_json::json!({
                        "type": "pipeline_error",
                        "error": e.detail(),
                        "error_type": e.error_type(),
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

    // Spawn blocking task — serializes ExecutorEvents as NDJSON.
    let allowlist_writer = Arc::clone(&session_allowlist);
    let writer_handle = tokio::task::spawn_blocking(move || {
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

    // Run execute_with_control on the current thread (block_in_place).
    let result = tokio::task::block_in_place(|| {
        ail_core::executor::execute_with_control(
            session,
            runner,
            &control,
            &disabled_steps,
            event_tx,
            hitl_rx,
        )
    });

    // Wait for writer task to drain.
    let _ = writer_handle.await;

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
                    "error": e.detail(),
                    "error_type": e.error_type(),
                }),
            );
            let _ = writeln!(out);
            std::process::exit(1);
        }
    }
}
