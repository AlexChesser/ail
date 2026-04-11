use ail_core::runner::{InvokeOptions, Runner};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use crate::control_bridge;

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

    let pending_permission = control_bridge::make_pending_perm();
    let session_allowlist = control_bridge::make_allowlist();

    let responder = control_bridge::make_allowlist_responder(
        Arc::clone(&pending_permission),
        Arc::clone(&session_allowlist),
    );

    control_bridge::spawn_stdin_reader_once(
        hitl_tx,
        Arc::clone(&pending_permission),
        Arc::clone(&session_allowlist),
        Arc::clone(&pause_requested),
        Arc::clone(&kill_requested),
    );

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
        let fwd_handle =
            control_bridge::spawn_runner_event_writer(runner_rx, Arc::clone(&session_allowlist));

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
    let disabled_steps = std::collections::HashSet::new();

    // Execute pipeline steps with event streaming.
    let (event_tx, event_rx) = mpsc::channel();

    let writer_handle =
        control_bridge::spawn_executor_event_writer(event_rx, Some(Arc::clone(&session_allowlist)));

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
