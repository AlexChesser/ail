//! `ail logs` subcommand: query and display execution logs.

use ail_core::logs::{query_logs, LogQuery, SessionSummary};

use crate::cli::OutputFormat;

/// Entry point for `ail logs`. Handles both one-shot and tail modes.
pub fn run_logs_command(
    session: Option<String>,
    query: Option<String>,
    format: OutputFormat,
    tail: bool,
    limit: usize,
) {
    if tail {
        run_tail(session, query, format, limit);
    } else {
        run_once(session, query, format, limit);
    }
}

fn run_once(session: Option<String>, query: Option<String>, format: OutputFormat, limit: usize) {
    let q = LogQuery {
        session_prefix: session,
        fts_query: query,
        limit,
    };
    match query_logs(&q) {
        Ok(results) => print_results(&results, format),
        Err(e) => {
            eprintln!("Error querying logs: {e}");
            std::process::exit(1);
        }
    }
}

fn run_tail(session: Option<String>, query: Option<String>, format: OutputFormat, limit: usize) {
    // Track the latest started_at we've printed so we only show new sessions.
    let mut last_seen_started_at: Option<i64> = None;

    loop {
        let q = LogQuery {
            session_prefix: session.clone(),
            fts_query: query.clone(),
            limit,
        };
        match query_logs(&q) {
            Ok(results) => {
                // Filter to sessions newer than last_seen_started_at.
                let new_entries: Vec<&SessionSummary> = results
                    .iter()
                    .filter(|s| {
                        if let (Some(started), Some(last)) = (s.started_at, last_seen_started_at) {
                            started > last
                        } else {
                            last_seen_started_at.is_none()
                        }
                    })
                    .collect();

                if !new_entries.is_empty() {
                    // Update our high-water mark.
                    if let Some(max_ts) = new_entries.iter().filter_map(|s| s.started_at).max() {
                        last_seen_started_at = Some(max_ts);
                    }
                    print_results(new_entries.as_slice(), format);
                } else if last_seen_started_at.is_none() {
                    // First poll, nothing yet — mark so we don't re-print on next.
                    last_seen_started_at = Some(0);
                }
            }
            Err(e) => {
                eprintln!("Error querying logs: {e}");
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn print_results(results: &[impl std::borrow::Borrow<SessionSummary>], format: OutputFormat) {
    match format {
        OutputFormat::Text => {
            for item in results {
                print_session_text(item.borrow());
            }
        }
        OutputFormat::Json => {
            for item in results {
                print_session_json(item.borrow());
            }
        }
    }
}

fn print_session_text(session: &SessionSummary) {
    print!("{}", build_session_text(session));
}

/// Build the text representation of a session summary.
///
/// Extracted for unit testing without I/O side effects.
pub(crate) fn build_session_text(session: &SessionSummary) -> String {
    let started = session
        .started_at
        .map(format_ts_ms)
        .unwrap_or_else(|| "unknown".to_string());
    let status = session.status.as_deref().unwrap_or("unknown");
    let cost = session
        .total_cost_usd
        .map(|c| format!("  cost: ${c:.4}"))
        .unwrap_or_default();
    let step_count = session.steps.len();

    let mut out = format!(
        "[{started}] run_id: {}  status: {status}{cost}  steps: {step_count}\n",
        &session.run_id,
    );

    for step in &session.steps {
        let tokens = match (step.input_tokens, step.output_tokens) {
            (Some(i), Some(o)) => format!("  {i}in/{o}out"),
            _ => String::new(),
        };
        let step_cost = step
            .cost_usd
            .map(|c| format!("  ${c:.4}"))
            .unwrap_or_default();

        out.push_str(&format!(
            "  {:<16} {:<12}{step_cost}{tokens}\n",
            step.step_id, step.event_type,
        ));
    }
    out.push('\n');
    out
}

fn print_session_json(session: &SessionSummary) {
    println!("{}", build_session_json(session));
}

/// Build the JSON value for a session summary.
///
/// Extracted for unit testing without I/O side effects.
pub(crate) fn build_session_json(session: &SessionSummary) -> serde_json::Value {
    // Serialise manually so we don't need serde on the core structs.
    let steps: Vec<serde_json::Value> = session
        .steps
        .iter()
        .map(|s| {
            serde_json::json!({
                "step_id": s.step_id,
                "event_type": s.event_type,
                "prompt": s.prompt,
                "response": s.response,
                "cost_usd": s.cost_usd,
                "input_tokens": s.input_tokens,
                "output_tokens": s.output_tokens,
                "thinking": s.thinking,
                "recorded_at": s.recorded_at,
                "latency_ms": s.latency_ms,
            })
        })
        .collect();

    serde_json::json!({
        "run_id": session.run_id,
        "pipeline_source": session.pipeline_source,
        "started_at": session.started_at,
        "completed_at": session.completed_at,
        "total_cost_usd": session.total_cost_usd,
        "status": session.status,
        "steps": steps,
    })
}

/// Format a Unix millisecond timestamp as `YYYY-MM-DD HH:MM:SS` in local time,
/// falling back to the raw ms value if conversion fails.
pub(crate) fn format_ts_ms(ms: i64) -> String {
    // Convert ms → seconds for std; use simple arithmetic to avoid pulling in chrono.
    let secs = ms / 1000;
    // std doesn't have localtime; format as UTC for portability.
    match std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs as u64))
    {
        Some(st) => {
            // Format via the Debug impl (not ideal) — use a manual calculation instead.
            let _ = st; // suppress unused warning
            format_utc_secs(secs)
        }
        None => ms.to_string(),
    }
}

/// Format UTC seconds as `YYYY-MM-DD HH:MM:SS` without external crates.
pub(crate) fn format_utc_secs(secs: i64) -> String {
    // Algorithm: civil_from_days (Howard Hinnant's approach)
    let secs_in_day = 86400i64;
    let z = secs / secs_in_day;
    let time_of_day = secs.rem_euclid(secs_in_day);

    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    // Days since 1970-01-01 to calendar date.
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mon = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mon <= 2 { y + 1 } else { y };

    format!("{yr:04}-{mon:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ail_core::logs::{SessionSummary, StepSummary};

    // ── format_utc_secs ──────────────────────────────────────────────────────

    #[test]
    fn format_utc_secs_unix_epoch() {
        assert_eq!(format_utc_secs(0), "1970-01-01 00:00:00");
    }

    #[test]
    fn format_utc_secs_known_date() {
        // 2024-01-15 11:30:45 UTC = 1705318245
        assert_eq!(format_utc_secs(1_705_318_245), "2024-01-15 11:30:45");
    }

    #[test]
    fn format_utc_secs_midnight() {
        // 2000-01-01 00:00:00 UTC = 946684800
        assert_eq!(format_utc_secs(946_684_800), "2000-01-01 00:00:00");
    }

    #[test]
    fn format_utc_secs_end_of_year() {
        // 2023-12-31 23:59:59 UTC = 1704067199
        assert_eq!(format_utc_secs(1_704_067_199), "2023-12-31 23:59:59");
    }

    #[test]
    fn format_utc_secs_leap_year_feb29() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        assert_eq!(format_utc_secs(1_709_164_800), "2024-02-29 00:00:00");
    }

    // ── format_ts_ms ─────────────────────────────────────────────────────────

    #[test]
    fn format_ts_ms_strips_sub_second() {
        // 1705318245000 ms = 2024-01-15 11:30:45 UTC
        assert_eq!(format_ts_ms(1_705_318_245_000), "2024-01-15 11:30:45");
    }

    #[test]
    fn format_ts_ms_sub_second_ignored() {
        // Adding 999ms should still produce the same second.
        assert_eq!(
            format_ts_ms(1_705_318_245_999),
            format_ts_ms(1_705_318_245_000)
        );
    }

    #[test]
    fn format_ts_ms_zero() {
        assert_eq!(format_ts_ms(0), "1970-01-01 00:00:00");
    }

    // ── build_session_text ───────────────────────────────────────────────────

    fn make_session(
        run_id: &str,
        status: Option<&str>,
        started_at: Option<i64>,
        total_cost_usd: Option<f64>,
        steps: Vec<StepSummary>,
    ) -> SessionSummary {
        SessionSummary {
            run_id: run_id.to_string(),
            pipeline_source: None,
            started_at,
            completed_at: None,
            total_cost_usd,
            status: status.map(|s| s.to_string()),
            steps,
        }
    }

    fn make_step(
        step_id: &str,
        event_type: &str,
        cost_usd: Option<f64>,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
    ) -> StepSummary {
        StepSummary {
            step_id: step_id.to_string(),
            event_type: event_type.to_string(),
            prompt: None,
            response: None,
            cost_usd,
            input_tokens,
            output_tokens,
            thinking: None,
            recorded_at: 0,
            latency_ms: None,
        }
    }

    #[test]
    fn session_text_contains_run_id() {
        let session = make_session("run-abc123", Some("completed"), None, None, vec![]);
        let text = build_session_text(&session);
        assert!(
            text.contains("run-abc123"),
            "expected run_id in output: {text}"
        );
    }

    #[test]
    fn session_text_contains_status() {
        let session = make_session("run-1", Some("completed"), None, None, vec![]);
        let text = build_session_text(&session);
        assert!(
            text.contains("status: completed"),
            "expected status in output: {text}"
        );
    }

    #[test]
    fn session_text_unknown_status_when_missing() {
        let session = make_session("run-1", None, None, None, vec![]);
        let text = build_session_text(&session);
        assert!(
            text.contains("status: unknown"),
            "expected unknown status: {text}"
        );
    }

    #[test]
    fn session_text_cost_formatted_as_usd() {
        let session = make_session("run-1", Some("ok"), None, Some(0.0123), vec![]);
        let text = build_session_text(&session);
        assert!(
            text.contains("cost: $0.0123"),
            "expected formatted cost: {text}"
        );
    }

    #[test]
    fn session_text_no_cost_when_missing() {
        let session = make_session("run-1", Some("ok"), None, None, vec![]);
        let text = build_session_text(&session);
        assert!(!text.contains("cost:"), "expected no cost field: {text}");
    }

    #[test]
    fn session_text_step_count() {
        let steps = vec![
            make_step("invocation", "step_completed", None, None, None),
            make_step("review", "step_completed", None, None, None),
        ];
        let session = make_session("run-1", Some("ok"), None, None, steps);
        let text = build_session_text(&session);
        assert!(text.contains("steps: 2"), "expected step count: {text}");
    }

    #[test]
    fn session_text_empty_steps() {
        let session = make_session("run-1", Some("ok"), None, None, vec![]);
        let text = build_session_text(&session);
        assert!(text.contains("steps: 0"), "expected zero steps: {text}");
    }

    #[test]
    fn session_text_step_tokens_formatted() {
        let steps = vec![make_step(
            "invocation",
            "step_completed",
            None,
            Some(100),
            Some(50),
        )];
        let session = make_session("run-1", Some("ok"), None, None, steps);
        let text = build_session_text(&session);
        assert!(
            text.contains("100in/50out"),
            "expected token counts: {text}"
        );
    }

    #[test]
    fn session_text_step_cost_formatted() {
        let steps = vec![make_step(
            "invocation",
            "step_completed",
            Some(0.0042),
            None,
            None,
        )];
        let session = make_session("run-1", Some("ok"), None, None, steps);
        let text = build_session_text(&session);
        assert!(text.contains("$0.0042"), "expected step cost: {text}");
    }

    #[test]
    fn session_text_unknown_timestamp_when_missing() {
        let session = make_session("run-1", Some("ok"), None, None, vec![]);
        let text = build_session_text(&session);
        assert!(
            text.contains("unknown"),
            "expected unknown timestamp: {text}"
        );
    }

    #[test]
    fn session_text_timestamp_formatted_when_present() {
        // 1705318245000 ms = 2024-01-15 11:30:45 UTC
        let session = make_session("run-1", Some("ok"), Some(1_705_318_245_000), None, vec![]);
        let text = build_session_text(&session);
        assert!(
            text.contains("2024-01-15 11:30:45"),
            "expected formatted timestamp: {text}"
        );
    }

    // ── build_session_json ───────────────────────────────────────────────────

    #[test]
    fn session_json_has_required_top_level_keys() {
        let session = make_session(
            "run-xyz",
            Some("completed"),
            Some(1_000_000),
            Some(1.5),
            vec![],
        );
        let json = build_session_json(&session);
        assert_eq!(json["run_id"], "run-xyz");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["started_at"], 1_000_000i64);
        assert!((json["total_cost_usd"].as_f64().unwrap() - 1.5).abs() < 1e-9);
        assert!(json["steps"].is_array());
    }

    #[test]
    fn session_json_empty_steps_array() {
        let session = make_session("run-1", None, None, None, vec![]);
        let json = build_session_json(&session);
        assert_eq!(json["steps"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn session_json_null_fields_when_missing() {
        let session = make_session("run-1", None, None, None, vec![]);
        let json = build_session_json(&session);
        assert!(json["status"].is_null());
        assert!(json["started_at"].is_null());
        assert!(json["total_cost_usd"].is_null());
        assert!(json["pipeline_source"].is_null());
        assert!(json["completed_at"].is_null());
    }

    #[test]
    fn session_json_step_fields_present() {
        let steps = vec![make_step(
            "invocation",
            "step_completed",
            Some(0.01),
            Some(10),
            Some(20),
        )];
        let session = make_session("run-1", Some("ok"), None, None, steps);
        let json = build_session_json(&session);
        let step = &json["steps"][0];
        assert_eq!(step["step_id"], "invocation");
        assert_eq!(step["event_type"], "step_completed");
        assert!((step["cost_usd"].as_f64().unwrap() - 0.01).abs() < 1e-9);
        assert_eq!(step["input_tokens"], 10i64);
        assert_eq!(step["output_tokens"], 20i64);
    }

    #[test]
    fn session_json_multiple_steps() {
        let steps = vec![
            make_step("invocation", "step_completed", None, None, None),
            make_step("review", "step_failed", None, None, None),
        ];
        let session = make_session("run-1", Some("failed"), None, None, steps);
        let json = build_session_json(&session);
        assert_eq!(json["steps"].as_array().unwrap().len(), 2);
        assert_eq!(json["steps"][1]["step_id"], "review");
        assert_eq!(json["steps"][1]["event_type"], "step_failed");
    }
}
