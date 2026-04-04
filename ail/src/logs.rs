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

    println!(
        "[{started}] run_id: {}  status: {status}{cost}  steps: {step_count}",
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

        println!(
            "  {:<16} {:<12}{step_cost}{tokens}",
            step.step_id, step.event_type,
        );
    }
    println!();
}

fn print_session_json(session: &SessionSummary) {
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

    let obj = serde_json::json!({
        "run_id": session.run_id,
        "pipeline_source": session.pipeline_source,
        "started_at": session.started_at,
        "completed_at": session.completed_at,
        "total_cost_usd": session.total_cost_usd,
        "status": session.status,
        "steps": steps,
    });

    println!("{obj}");
}

/// Format a Unix millisecond timestamp as `YYYY-MM-DD HH:MM:SS` in local time,
/// falling back to the raw ms value if conversion fails.
fn format_ts_ms(ms: i64) -> String {
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
fn format_utc_secs(secs: i64) -> String {
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
