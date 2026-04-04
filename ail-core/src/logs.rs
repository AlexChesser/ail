//! Query interface for ail execution logs stored in SQLite.
//!
//! The primary entry point is [`query_logs`] (uses `~/.ail/projects/<sha1_of_cwd>/ail.db`)
//! and [`query_logs_at`] (test-friendly explicit path variant).

#![allow(clippy::result_large_err)]

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

use crate::error::{error_types, AilError};
use crate::runner::ToolEvent;
use crate::session::sqlite_provider::db_path;

/// Parameters for a log query.
pub struct LogQuery {
    /// Filter sessions where `run_id LIKE 'prefix%'`.
    pub session_prefix: Option<String>,
    /// Full-text search across step content (uses FTS5 `traces_fts` table).
    pub fts_query: Option<String>,
    /// Maximum number of sessions to return.
    pub limit: usize,
}

/// Summary of a single pipeline run session.
#[derive(Debug)]
pub struct SessionSummary {
    pub run_id: String,
    pub pipeline_source: Option<String>,
    /// Unix epoch milliseconds.
    pub started_at: Option<i64>,
    /// Unix epoch milliseconds.
    pub completed_at: Option<i64>,
    pub total_cost_usd: Option<f64>,
    pub status: Option<String>,
    pub steps: Vec<StepSummary>,
}

/// Summary of a single step within a session.
#[derive(Debug)]
pub struct StepSummary {
    pub step_id: String,
    pub event_type: String,
    pub response: Option<String>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub thinking: Option<String>,
    /// Unix epoch milliseconds.
    pub recorded_at: i64,
    /// Duration from `step_started` to `step_completed` in milliseconds.
    /// `None` when either event is missing for this step.
    pub latency_ms: Option<i64>,
}

/// Query logs from the default DB location (`~/.ail/projects/<sha1_of_cwd>/ail.db`).
///
/// Returns an empty `Vec` if the database file does not exist.
pub fn query_logs(query: &LogQuery) -> Result<Vec<SessionSummary>, AilError> {
    query_logs_at(query, &db_path())
}

/// Query logs from an explicit DB path. Intended for tests and tool access.
///
/// Returns an empty `Vec` if `db_path` does not exist.
pub fn query_logs_at(query: &LogQuery, db_path: &Path) -> Result<Vec<SessionSummary>, AilError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let conn = Connection::open(db_path).map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to open log database",
        detail: e.to_string(),
        context: None,
    })?;

    // Collect run_ids that match the FTS query, if provided.
    let fts_run_ids: Option<Vec<String>> = if let Some(ref fts) = query.fts_query {
        let mut stmt = conn
            .prepare("SELECT DISTINCT run_id FROM traces_fts WHERE traces_fts MATCH ?1 LIMIT 1000")
            .map_err(|e| AilError {
                error_type: error_types::PIPELINE_ABORTED,
                title: "Failed to prepare FTS query",
                detail: e.to_string(),
                context: None,
            })?;
        let ids: Result<Vec<String>, _> = stmt
            .query_map(params![fts], |row| row.get(0))
            .map_err(|e| AilError {
                error_type: error_types::PIPELINE_ABORTED,
                title: "Failed to execute FTS query",
                detail: e.to_string(),
                context: None,
            })?
            .collect::<Result<Vec<_>, _>>();
        let ids = ids.map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to collect FTS results",
            detail: e.to_string(),
            context: None,
        })?;
        Some(ids)
    } else {
        None
    };

    // Build the sessions query dynamically based on filters.
    let sessions = load_sessions(&conn, query, fts_run_ids.as_deref())?;

    // Load steps for each session.
    let mut results = Vec::with_capacity(sessions.len());
    for mut session in sessions {
        session.steps = load_steps(&conn, &session.run_id)?;
        results.push(session);
    }

    Ok(results)
}

fn load_sessions(
    conn: &Connection,
    query: &LogQuery,
    fts_run_ids: Option<&[String]>,
) -> Result<Vec<SessionSummary>, AilError> {
    // We build the query with positional parameters; the FTS filter uses an IN clause
    // with individually bound values when present. rusqlite doesn't support binding
    // a slice directly, so we generate placeholders.
    let mut conditions: Vec<String> = Vec::new();
    let mut position = 1usize;

    // Session prefix filter.
    let prefix_param = query.session_prefix.as_ref().map(|p| format!("{p}%"));
    if prefix_param.is_some() {
        conditions.push(format!("run_id LIKE ?{position}"));
        position += 1;
    }

    // FTS filter: restrict to matching run_ids.
    let fts_placeholders: Option<String> = fts_run_ids.map(|ids| {
        let ph: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", position + i))
            .collect();
        let clause = format!("run_id IN ({})", ph.join(", "));
        position += ids.len();
        clause
    });
    if let Some(ref clause) = fts_placeholders {
        conditions.push(clause.clone());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT run_id, pipeline_source, started_at, completed_at, total_cost_usd, status
         FROM sessions
         {where_clause}
         ORDER BY started_at DESC
         LIMIT ?{position}"
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to prepare sessions query",
        detail: e.to_string(),
        context: None,
    })?;

    // Bind parameters in order.
    let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(ref prefix) = prefix_param {
        param_values.push(Box::new(prefix.clone()));
    }
    if let Some(ids) = fts_run_ids {
        for id in ids {
            param_values.push(Box::new(id.clone()));
        }
    }
    param_values.push(Box::new(query.limit as i64));

    let refs: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|b| b.as_ref()).collect();

    let rows: Result<Vec<SessionSummary>, _> = stmt
        .query_map(refs.as_slice(), |row| {
            Ok(SessionSummary {
                run_id: row.get(0)?,
                pipeline_source: row.get(1)?,
                started_at: row.get(2)?,
                completed_at: row.get(3)?,
                total_cost_usd: row.get(4)?,
                status: row.get(5)?,
                steps: Vec::new(),
            })
        })
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to query sessions",
            detail: e.to_string(),
            context: None,
        })?
        .collect::<Result<Vec<_>, _>>();

    rows.map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to collect session rows",
        detail: e.to_string(),
        context: None,
    })
}

/// Raw step row read from the database before latency enrichment.
struct RawStepRow {
    step_id: String,
    event_type: String,
    response: Option<String>,
    cost_usd: Option<f64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    thinking: Option<String>,
    recorded_at: i64,
}

fn load_steps(conn: &Connection, run_id: &str) -> Result<Vec<StepSummary>, AilError> {
    let mut stmt = conn
        .prepare(
            "SELECT step_id, event_type, response, cost_usd, input_tokens,
                    output_tokens, thinking, recorded_at
             FROM steps
             WHERE run_id = ?1
             ORDER BY recorded_at ASC",
        )
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to prepare steps query",
            detail: e.to_string(),
            context: None,
        })?;

    let rows: Result<Vec<RawStepRow>, _> = stmt
        .query_map(params![run_id], |row| {
            Ok(RawStepRow {
                step_id: row.get(0)?,
                event_type: row.get(1)?,
                response: row.get(2)?,
                cost_usd: row.get(3)?,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                thinking: row.get(6)?,
                recorded_at: row.get(7)?,
            })
        })
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to query steps",
            detail: e.to_string(),
            context: None,
        })?
        .collect::<Result<Vec<_>, _>>();

    let raw_rows = rows.map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to collect step rows",
        detail: e.to_string(),
        context: None,
    })?;

    // Build a map of step_id → started_at timestamp for latency computation.
    // A step may have multiple rows (step_started + step_completed); we pair them.
    use std::collections::HashMap;
    let mut started_at_map: HashMap<String, i64> = HashMap::new();
    for row in &raw_rows {
        if row.event_type == "step_started" {
            started_at_map.insert(row.step_id.clone(), row.recorded_at);
        }
    }

    let steps: Vec<StepSummary> = raw_rows
        .into_iter()
        .map(|row| {
            let latency_ms = if row.event_type == "step_completed" {
                started_at_map
                    .get(&row.step_id)
                    .map(|started| row.recorded_at - started)
            } else {
                None
            };
            StepSummary {
                step_id: row.step_id,
                event_type: row.event_type,
                response: row.response,
                cost_usd: row.cost_usd,
                input_tokens: row.input_tokens,
                output_tokens: row.output_tokens,
                thinking: row.thinking,
                recorded_at: row.recorded_at,
                latency_ms,
            }
        })
        .collect();

    Ok(steps)
}

/// Represents a single step row from the database for use in formatters.
#[derive(Debug, Clone)]
pub struct StepRow {
    pub step_id: String,
    pub event_type: String,
    pub prompt: Option<String>,
    pub response: Option<String>,
    pub thinking: Option<String>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub recorded_at: i64,
    /// Tool call and result events for this step, ordered by seq.
    pub tool_events: Vec<ToolEvent>,
}

/// Query steps for a specific run from the default database location.
///
/// Returns steps ordered by `recorded_at`.
pub fn get_run_steps(run_id: &str) -> Result<Vec<StepRow>, AilError> {
    get_run_steps_at(run_id, &db_path())
}

/// Query steps for a specific run from an explicit database path.
pub fn get_run_steps_at(run_id: &str, db_path: &Path) -> Result<Vec<StepRow>, AilError> {
    if !db_path.exists() {
        return Err(AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Database not found",
            detail: "No ail database found at this location".to_string(),
            context: None,
        });
    }

    let conn = Connection::open(db_path).map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to open log database",
        detail: e.to_string(),
        context: None,
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT step_id, event_type, prompt, response, thinking, cost_usd, input_tokens,
                    output_tokens, stdout, stderr, exit_code, recorded_at
             FROM steps
             WHERE run_id = ?1
             ORDER BY recorded_at ASC",
        )
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to prepare steps query",
            detail: e.to_string(),
            context: None,
        })?;

    let rows: Result<Vec<StepRow>, _> = stmt
        .query_map(params![run_id], |row| {
            Ok(StepRow {
                step_id: row.get(0)?,
                event_type: row.get(1)?,
                prompt: row.get(2)?,
                response: row.get(3)?,
                thinking: row.get(4)?,
                cost_usd: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                stdout: row.get(8)?,
                stderr: row.get(9)?,
                exit_code: row.get(10)?,
                recorded_at: row.get(11)?,
                tool_events: vec![],
            })
        })
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to query steps",
            detail: e.to_string(),
            context: None,
        })?
        .collect::<Result<Vec<_>, _>>();

    let mut rows = rows.map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to collect step rows",
        detail: e.to_string(),
        context: None,
    })?;

    // Populate tool_events from run_events table, if it exists.
    // Guard with a table-existence check so old databases (without run_events) still work.
    let run_events_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='run_events'",
            [],
            |row| row.get::<_, u32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    if run_events_exists {
        let mut ev_stmt = conn
            .prepare(
                "SELECT step_id, seq, event_type, tool_name, tool_id, content_json
                 FROM run_events WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(|e| AilError {
                error_type: error_types::PIPELINE_ABORTED,
                title: "Failed to prepare run_events query",
                detail: e.to_string(),
                context: None,
            })?;

        let event_rows: Result<Vec<(String, ToolEvent)>, _> = ev_stmt
            .query_map(params![run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ToolEvent {
                        tool_id: row.get(4)?,
                        tool_name: row.get(3)?,
                        event_type: row.get(2)?,
                        content_json: row.get(5)?,
                        seq: row.get(1)?,
                    },
                ))
            })
            .map_err(|e| AilError {
                error_type: error_types::PIPELINE_ABORTED,
                title: "Failed to query run_events",
                detail: e.to_string(),
                context: None,
            })?
            .collect::<Result<Vec<_>, _>>();

        let event_rows = event_rows.map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to collect run_events rows",
            detail: e.to_string(),
            context: None,
        })?;

        // Build a map of step_id → Vec<ToolEvent>.
        let mut ev_map: HashMap<String, Vec<ToolEvent>> = HashMap::new();
        for (step_id, te) in event_rows {
            ev_map.entry(step_id).or_default().push(te);
        }

        // Attach tool events to matching StepRow entries.
        for row in &mut rows {
            if let Some(events) = ev_map.remove(&row.step_id) {
                row.tool_events = events;
            }
        }
    }

    Ok(rows)
}

/// Get the most recent run ID for the current working directory.
///
/// Computes the SHA-1 hash of the absolute CWD path and queries the database.
pub fn get_latest_run_id_for_cwd() -> Result<Option<String>, AilError> {
    use sha1::{Digest, Sha1};
    use std::path::PathBuf;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut hasher = Sha1::new();
    hasher.update(cwd.to_string_lossy().as_bytes());
    let project_hash = format!("{:x}", hasher.finalize());

    get_latest_run_id(&project_hash)
}

/// Get the most recent run ID for a given project (identified by SHA-1 of absolute CWD path).
pub fn get_latest_run_id(project_sha1: &str) -> Result<Option<String>, AilError> {
    get_latest_run_id_at(project_sha1, &db_path())
}

/// Get the most recent run ID for a given project from an explicit database path.
pub fn get_latest_run_id_at(
    project_sha1: &str,
    db_path: &Path,
) -> Result<Option<String>, AilError> {
    if !db_path.exists() {
        return Ok(None);
    }

    let conn = Connection::open(db_path).map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to open log database",
        detail: e.to_string(),
        context: None,
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT run_id FROM sessions
             WHERE project_hash = ?1
             ORDER BY started_at DESC
             LIMIT 1",
        )
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to prepare query",
            detail: e.to_string(),
            context: None,
        })?;

    let run_id: Option<String> = stmt
        .query_row(params![project_sha1], |row| row.get(0))
        .optional()
        .map_err(|e| AilError {
            error_type: error_types::PIPELINE_ABORTED,
            title: "Failed to query latest run",
            detail: e.to_string(),
            context: None,
        })?;

    Ok(run_id)
}
