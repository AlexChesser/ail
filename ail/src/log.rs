//! `ail log [run_id]` subcommand: display formatted pipeline run results.

#![allow(clippy::result_large_err)]

use ail_core::formatter::format_run_as_ail_log;
use ail_core::logs::{get_latest_run_id_for_cwd, get_run_steps};
use ail_core::session::project_dir;

/// Returns `true` if `format` is a recognised output format name.
pub(crate) fn validate_format(format: &str) -> bool {
    matches!(format, "markdown" | "json" | "raw")
}

/// Build a single NDJSON object for one step row.
pub(crate) fn build_step_json(run_id: &str, step: &ail_core::logs::StepRow) -> serde_json::Value {
    serde_json::json!({
        "run_id": run_id,
        "step_id": step.step_id,
        "event_type": step.event_type,
        "response": step.response,
        "thinking": step.thinking,
        "cost_usd": step.cost_usd,
        "input_tokens": step.input_tokens,
        "output_tokens": step.output_tokens,
        "recorded_at": step.recorded_at,
    })
}

/// Format a slice of step rows as a string for the given format.
/// Handles `"markdown"` (via `format_run_as_ail_log`) and `"json"` (NDJSON).
/// Returns an empty string for `"raw"` — callers must handle raw separately.
pub(crate) fn format_steps(
    steps: &[ail_core::logs::StepRow],
    run_id: &str,
    format: &str,
) -> String {
    match format {
        "markdown" => format_run_as_ail_log(steps),
        "json" => {
            let mut out = String::new();
            for step in steps {
                out.push_str(
                    &serde_json::to_string(&build_step_json(run_id, step)).unwrap_or_default(),
                );
                out.push('\n');
            }
            out
        }
        _ => String::new(),
    }
}

/// Return the subset of `steps` whose `recorded_at` timestamp is strictly after `after`.
pub(crate) fn filter_new_steps(
    steps: &[ail_core::logs::StepRow],
    after: i64,
) -> Vec<ail_core::logs::StepRow> {
    steps
        .iter()
        .filter(|s| s.recorded_at > after)
        .cloned()
        .collect()
}

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
                eprintln!("ail: {}", e.detail());
                std::process::exit(2);
            }
        },
    };

    // Validate format argument.
    if !validate_format(format) {
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
            if format == "raw" {
                if let Err(e) = print_raw_jsonl(run_id) {
                    eprintln!("ail: {}", e.detail());
                    std::process::exit(2);
                }
            } else {
                print!("{}", format_steps(&steps, run_id, format));
            }
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("ail: {}", e.detail());
            // If the run_id doesn't exist, exit with code 1. If DB error, code 2.
            let exit_code = if e.detail().contains("not found") {
                1
            } else {
                2
            };
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

            if format == "raw" {
                if let Err(e) = print_raw_jsonl(run_id) {
                    eprintln!("ail: {}", e.detail());
                    std::process::exit(2);
                }
                if let Ok(s) = get_run_steps(run_id) {
                    if let Some(last) = s.last() {
                        last_recorded_at = last.recorded_at;
                    }
                }
            } else {
                print!("{}", format_steps(&steps, run_id, format));
                if let Some(last) = steps.last() {
                    last_recorded_at = last.recorded_at;
                }
            }

            let mut is_final = ail_core::logs::is_run_complete(&steps);

            // Polling loop: exit when final step completes.
            while !is_final {
                std::thread::sleep(std::time::Duration::from_millis(500));

                // Retry loop: up to 3 retries on SQLITE_BUSY
                let mut tick_success = false;
                for attempt in 0..=2 {
                    match get_run_steps(run_id) {
                        Ok(new_steps) => {
                            let additional = filter_new_steps(&new_steps, last_recorded_at);

                            if !additional.is_empty() {
                                if format != "raw" {
                                    print!("{}", format_steps(&additional, run_id, format));
                                }
                                // raw follow mode skips incremental output (re-print on next tick).
                                if let Some(last_step) = additional.last() {
                                    last_recorded_at = last_step.recorded_at;
                                }
                            }

                            is_final = ail_core::logs::is_run_complete(&new_steps);
                            tick_success = true;
                            break;
                        }
                        Err(e) => {
                            // Check if error is SQLITE_BUSY (or similar transient DB error)
                            let is_transient = e.detail().contains("database is locked")
                                || e.detail().contains("SQLITE_BUSY");

                            if is_transient && attempt < 2 {
                                // Retry with 50ms backoff
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            } else if !is_transient {
                                // Non-transient error: exit immediately with code 1
                                eprintln!("ail: database error: {}", e.detail());
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
            eprintln!("ail: {}", e.detail());
            std::process::exit(2);
        }
    }
}

/// Print raw JSONL entries from the run's stored JSONL file.
fn print_raw_jsonl(run_id: &str) -> Result<(), ail_core::error::AilError> {
    let run_path = project_dir().join("runs").join(format!("{run_id}.jsonl"));

    if !run_path.exists() {
        return Err(ail_core::error::AilError::PipelineAborted {
            detail: format!("No run file found at {}", run_path.display()),
            context: None,
        });
    }

    let content = std::fs::read_to_string(&run_path).map_err(|e| {
        ail_core::error::AilError::PipelineAborted {
            detail: e.to_string(),
            context: None,
        }
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

    fn make_step_row_at(step_id: &str, event_type: &str, recorded_at: i64) -> StepRow {
        StepRow {
            recorded_at,
            ..make_step_row(step_id, event_type)
        }
    }

    // ── validate_format ─────────────────────────────────────────────────────

    #[test]
    fn validate_format_accepts_markdown() {
        assert!(validate_format("markdown"));
    }

    #[test]
    fn validate_format_accepts_json() {
        assert!(validate_format("json"));
    }

    #[test]
    fn validate_format_accepts_raw() {
        assert!(validate_format("raw"));
    }

    #[test]
    fn validate_format_rejects_unknown() {
        assert!(!validate_format("html"));
        assert!(!validate_format(""));
        assert!(!validate_format("Markdown"));
    }

    // ── build_step_json ──────────────────────────────────────────────────────

    #[test]
    fn build_step_json_includes_run_id() {
        let step = make_step_row("invocation", "step_completed");
        let obj = build_step_json("run-abc", &step);
        assert_eq!(obj["run_id"], "run-abc");
    }

    #[test]
    fn build_step_json_includes_step_id_and_event_type() {
        let step = make_step_row("review", "step_started");
        let obj = build_step_json("r1", &step);
        assert_eq!(obj["step_id"], "review");
        assert_eq!(obj["event_type"], "step_started");
    }

    #[test]
    fn build_step_json_null_optional_fields_when_absent() {
        let step = make_step_row("s", "step_completed");
        let obj = build_step_json("r", &step);
        assert!(obj["response"].is_null());
        assert!(obj["cost_usd"].is_null());
        assert!(obj["input_tokens"].is_null());
        assert!(obj["output_tokens"].is_null());
        assert!(obj["thinking"].is_null());
    }

    #[test]
    fn build_step_json_populated_optional_fields() {
        let step = StepRow {
            response: Some("looks good".to_string()),
            cost_usd: Some(0.005),
            input_tokens: Some(100),
            output_tokens: Some(50),
            recorded_at: 9999,
            ..make_step_row("s", "step_completed")
        };
        let obj = build_step_json("r", &step);
        assert_eq!(obj["response"], "looks good");
        assert_eq!(obj["cost_usd"], 0.005);
        assert_eq!(obj["input_tokens"], 100);
        assert_eq!(obj["output_tokens"], 50);
        assert_eq!(obj["recorded_at"], 9999);
    }

    #[test]
    fn build_step_json_is_valid_json() {
        let step = make_step_row("s", "step_completed");
        let obj = build_step_json("r", &step);
        let serialized = serde_json::to_string(&obj).expect("must serialize");
        let reparsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("must round-trip");
        assert_eq!(reparsed["run_id"], "r");
    }

    // ── format_steps ────────────────────────────────────────────────────────

    #[test]
    fn format_steps_json_empty_slice_returns_empty_string() {
        let out = format_steps(&[], "r", "json");
        assert_eq!(out, "");
    }

    #[test]
    fn format_steps_json_produces_one_line_per_step() {
        let steps = vec![
            make_step_row("a", "step_started"),
            make_step_row("b", "step_completed"),
        ];
        let out = format_steps(&steps, "run1", "json");
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn format_steps_json_each_line_is_valid_json_with_step_id() {
        let steps = vec![make_step_row("review", "step_completed")];
        let out = format_steps(&steps, "my-run", "json");
        let obj: serde_json::Value = serde_json::from_str(out.trim()).expect("must be valid JSON");
        assert_eq!(obj["step_id"], "review");
        assert_eq!(obj["run_id"], "my-run");
    }

    #[test]
    fn format_steps_markdown_returns_non_empty_for_steps() {
        let steps = vec![make_step_row("s", "step_completed")];
        let out = format_steps(&steps, "r", "markdown");
        assert!(!out.is_empty());
    }

    #[test]
    fn format_steps_unknown_format_returns_empty_string() {
        let steps = vec![make_step_row("s", "step_completed")];
        let out = format_steps(&steps, "r", "html");
        assert_eq!(out, "");
    }

    // ── filter_new_steps ────────────────────────────────────────────────────

    #[test]
    fn filter_new_steps_returns_all_when_after_is_zero() {
        let steps = vec![
            make_step_row_at("a", "step_started", 1),
            make_step_row_at("b", "step_completed", 2),
        ];
        let result = filter_new_steps(&steps, 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_new_steps_excludes_steps_at_or_before_cutoff() {
        let steps = vec![
            make_step_row_at("a", "step_started", 100),
            make_step_row_at("b", "step_completed", 200),
            make_step_row_at("c", "step_completed", 300),
        ];
        let result = filter_new_steps(&steps, 100);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].step_id, "b");
        assert_eq!(result[1].step_id, "c");
    }

    #[test]
    fn filter_new_steps_returns_empty_when_all_old() {
        let steps = vec![
            make_step_row_at("a", "step_started", 50),
            make_step_row_at("b", "step_completed", 100),
        ];
        let result = filter_new_steps(&steps, 100);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_new_steps_returns_empty_for_empty_slice() {
        let result = filter_new_steps(&[], 0);
        assert!(result.is_empty());
    }
}
