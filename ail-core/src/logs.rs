//! Query interface for ail execution logs stored in SQLite.
//!
//! The primary entry point is [`query_logs`] (uses `~/.ail/projects/<sha1_of_cwd>/ail.db`)
//! and [`query_logs_at`] (test-friendly explicit path variant).

#![allow(clippy::result_large_err)]

use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::{error_types, AilError};
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

    let rows: Result<Vec<StepSummary>, _> = stmt
        .query_map(params![run_id], |row| {
            Ok(StepSummary {
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

    rows.map_err(|e| AilError {
        error_type: error_types::PIPELINE_ABORTED,
        title: "Failed to collect step rows",
        detail: e.to_string(),
        context: None,
    })
}
