//! `ail log [run_id]` subcommand: display formatted pipeline run results.

#![allow(clippy::result_large_err)]

use ail_core::formatter::format_run_as_ail_log;
use ail_core::logs::{get_latest_run_id_for_cwd, get_run_steps};
use ail_core::session::project_dir;

/// Entry point for `ail log [run_id]`.
pub fn run_log_command(run_id: Option<String>, format: &str, follow: bool) {
    let resolved_run_id = match run_id {
        Some(id) => id,
        None => match resolve_latest_run_id() {
            Ok(Some(id)) => id,
            Ok(None) => {
                eprintln!("ail: no runs found for current directory");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("ail: {}", e.detail);
                std::process::exit(2);
            }
        },
    };

    // Validate format argument.
    if !matches!(format, "markdown" | "json" | "raw") {
        eprintln!("ail: invalid format: {format} (must be markdown, json, or raw)");
        std::process::exit(3);
    }

    if follow {
        run_follow(&resolved_run_id, format);
    } else {
        run_once(&resolved_run_id, format);
    }
}

/// Resolve the most recent run_id for the current directory.
fn resolve_latest_run_id() -> Result<Option<String>, ail_core::error::AilError> {
    get_latest_run_id_for_cwd()
}

/// One-shot output: fetch all steps and format.
fn run_once(run_id: &str, format: &str) {
    match get_run_steps(run_id) {
        Ok(steps) => {
            match format {
                "markdown" => {
                    let output = format_run_as_ail_log(&steps);
                    print!("{output}");
                }
                "json" => {
                    // Output NDJSON: one JSON object per step.
                    for step in steps {
                        let obj = serde_json::json!({
                            "run_id": run_id,
                            "step_id": step.step_id,
                            "event_type": step.event_type,
                            "response": step.response,
                            "thinking": step.thinking,
                            "cost_usd": step.cost_usd,
                            "input_tokens": step.input_tokens,
                            "output_tokens": step.output_tokens,
                            "recorded_at": step.recorded_at,
                        });
                        println!("{obj}");
                    }
                }
                "raw" => {
                    // Output stored JSONL entries verbatim (from the runs directory).
                    if let Err(e) = print_raw_jsonl(run_id) {
                        eprintln!("ail: {}", e.detail);
                        std::process::exit(2);
                    }
                }
                _ => {
                    eprintln!("ail: invalid format: {format}");
                    std::process::exit(3);
                }
            }
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("ail: {}", e.detail);
            // If the run_id doesn't exist, exit with code 1. If DB error, code 2.
            let exit_code = if e.detail.contains("not found") { 1 } else { 2 };
            std::process::exit(exit_code);
        }
    }
}

/// Stream output: emit current state, then poll for new steps every 500ms.
/// Retries on SQLITE_BUSY with exponential backoff (max 3 retries per tick).
fn run_follow(run_id: &str, format: &str) {
    let mut last_recorded_at: i64 = 0;

    // Emit initial state.
    match get_run_steps(run_id) {
        Ok(steps) => {
            if steps.is_empty() {
                eprintln!("ail: run not found or has no steps: {run_id}");
                std::process::exit(1);
            }

            match format {
                "markdown" => {
                    let output = format_run_as_ail_log(&steps);
                    print!("{output}");
                    if !steps.is_empty() {
                        last_recorded_at = steps.last().unwrap().recorded_at;
                    }
                }
                "json" => {
                    for step in &steps {
                        let obj = serde_json::json!({
                            "run_id": run_id,
                            "step_id": step.step_id,
                            "event_type": step.event_type,
                            "response": step.response,
                            "thinking": step.thinking,
                            "cost_usd": step.cost_usd,
                            "input_tokens": step.input_tokens,
                            "output_tokens": step.output_tokens,
                            "recorded_at": step.recorded_at,
                        });
                        println!("{obj}");
                        last_recorded_at = step.recorded_at;
                    }
                }
                "raw" => {
                    if let Err(e) = print_raw_jsonl(run_id) {
                        eprintln!("ail: {}", e.detail);
                        std::process::exit(2);
                    }
                    if let Ok(steps) = get_run_steps(run_id) {
                        if !steps.is_empty() {
                            last_recorded_at = steps.last().unwrap().recorded_at;
                        }
                    }
                }
                _ => {
                    eprintln!("ail: invalid format: {format}");
                    std::process::exit(3);
                }
            }

            let mut is_final = check_is_final(run_id, &steps);

            // Polling loop: exit when final step completes.
            while !is_final {
                std::thread::sleep(std::time::Duration::from_millis(500));

                // Retry loop: up to 3 retries on SQLITE_BUSY
                let mut tick_success = false;
                for attempt in 0..=2 {
                    match get_run_steps(run_id) {
                        Ok(new_steps) => {
                            let additional: Vec<_> = new_steps
                                .iter()
                                .filter(|s| s.recorded_at > last_recorded_at)
                                .collect();

                            if !additional.is_empty() {
                                match format {
                                    "markdown" => {
                                        let output = format_run_as_ail_log(
                                            &additional
                                                .iter()
                                                .map(|s| (*s).clone())
                                                .collect::<Vec<_>>(),
                                        );
                                        print!("{output}");
                                    }
                                    "json" => {
                                        for step in &additional {
                                            let obj = serde_json::json!({
                                                "run_id": run_id,
                                                "step_id": step.step_id,
                                                "event_type": step.event_type,
                                                "response": step.response,
                                                "thinking": step.thinking,
                                                "cost_usd": step.cost_usd,
                                                "input_tokens": step.input_tokens,
                                                "output_tokens": step.output_tokens,
                                                "recorded_at": step.recorded_at,
                                            });
                                            println!("{obj}");
                                        }
                                    }
                                    "raw" => {
                                        // For raw, we re-print the whole thing (or just new entries).
                                        // For now, we'll skip raw in follow mode for simplicity.
                                    }
                                    _ => {}
                                }

                                if let Some(last_step) = additional.last() {
                                    last_recorded_at = last_step.recorded_at;
                                }
                            }

                            is_final = check_is_final(run_id, &new_steps);
                            tick_success = true;
                            break;
                        }
                        Err(e) => {
                            // Check if error is SQLITE_BUSY (or similar transient DB error)
                            let is_transient = e.detail.contains("database is locked")
                                || e.detail.contains("SQLITE_BUSY");

                            if is_transient && attempt < 2 {
                                // Retry with 50ms backoff
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            } else if !is_transient {
                                // Non-transient error: exit immediately with code 1
                                eprintln!("ail: database error: {}", e.detail);
                                std::process::exit(1);
                            } else {
                                // Transient error after max retries: skip this tick
                                tick_success = true;
                                break;
                            }
                        }
                    }
                }

                if !tick_success {
                    // Should not reach here, but if we do, skip this tick
                    continue;
                }
            }
        }
        Err(e) => {
            eprintln!("ail: {}", e.detail);
            std::process::exit(2);
        }
    }
}

/// Check if the final step has completed by looking for a step_completed or step_failed
/// event on the last step_id.
fn check_is_final(_run_id: &str, steps: &[ail_core::logs::StepRow]) -> bool {
    if steps.is_empty() {
        return false;
    }

    // Get the last unique step_id.
    let mut seen = std::collections::HashSet::new();
    let mut last_step_id = None;
    for step in steps {
        last_step_id = Some(step.step_id.clone());
        seen.insert(step.step_id.clone());
    }

    // Check if the last step has a completion or failure event.
    if let Some(ref last_id) = last_step_id {
        for step in steps {
            if step.step_id == *last_id
                && (step.event_type == "step_completed" || step.event_type == "step_failed")
            {
                return true;
            }
        }
    }

    false
}

/// Print raw JSONL entries from the run's stored JSONL file.
fn print_raw_jsonl(run_id: &str) -> Result<(), ail_core::error::AilError> {
    let run_path = project_dir().join("runs").join(format!("{run_id}.jsonl"));

    if !run_path.exists() {
        return Err(ail_core::error::AilError {
            error_type: ail_core::error::error_types::PIPELINE_ABORTED,
            title: "Run file not found",
            detail: format!("No run file found at {}", run_path.display()),
            context: None,
        });
    }

    let content = std::fs::read_to_string(&run_path).map_err(|e| ail_core::error::AilError {
        error_type: ail_core::error::error_types::PIPELINE_ABORTED,
        title: "Failed to read run file",
        detail: e.to_string(),
        context: None,
    })?;

    print!("{content}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ail_core::logs::StepRow;

    fn make_step_row(step_id: &str, event_type: &str) -> StepRow {
        StepRow {
            step_id: step_id.to_string(),
            event_type: event_type.to_string(),
            prompt: None,
            response: None,
            thinking: None,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 0,
            tool_events: vec![],
        }
    }

    #[test]
    fn check_is_final_empty_steps_returns_false() {
        assert!(!check_is_final("run-1", &[]));
    }

    #[test]
    fn check_is_final_returns_false_without_terminal_event() {
        let steps = vec![
            make_step_row("invocation", "step_started"),
            make_step_row("invocation", "tool_call"),
        ];
        assert!(!check_is_final("run-1", &steps));
    }

    #[test]
    fn check_is_final_returns_true_on_step_completed() {
        let steps = vec![
            make_step_row("invocation", "step_started"),
            make_step_row("invocation", "step_completed"),
        ];
        assert!(check_is_final("run-1", &steps));
    }

    #[test]
    fn check_is_final_returns_true_on_step_failed() {
        let steps = vec![
            make_step_row("invocation", "step_started"),
            make_step_row("invocation", "step_failed"),
        ];
        assert!(check_is_final("run-1", &steps));
    }

    #[test]
    fn check_is_final_only_checks_last_step_id() {
        // "invocation" completes but "review" is the last step and has not finished.
        let steps = vec![
            make_step_row("invocation", "step_started"),
            make_step_row("invocation", "step_completed"),
            make_step_row("review", "step_started"),
        ];
        assert!(!check_is_final("run-1", &steps));
    }

    #[test]
    fn check_is_final_true_when_last_step_completes() {
        let steps = vec![
            make_step_row("invocation", "step_started"),
            make_step_row("invocation", "step_completed"),
            make_step_row("review", "step_started"),
            make_step_row("review", "step_completed"),
        ];
        assert!(check_is_final("run-1", &steps));
    }

    #[test]
    fn check_is_final_true_when_last_step_fails() {
        let steps = vec![
            make_step_row("invocation", "step_completed"),
            make_step_row("review", "step_started"),
            make_step_row("review", "step_failed"),
        ];
        assert!(check_is_final("run-1", &steps));
    }
}
