//! SQLite persistence backend for pipeline run logs.
//!
//! Stores sessions, steps, FTS-indexed trace content, and arbitrary key/value metadata
//! in a single SQLite database at `~/.ail/projects/<sha1_of_cwd>/ail.db`.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, Result as SqliteResult};
use serde_json::Value;

use super::log_provider::{project_dir, LogProvider};

/// SQLite-backed log provider.
///
/// Schema:
/// - `sessions` — one row per pipeline run
/// - `steps`    — one row per turn log entry
/// - `traces`   — FTS5 virtual table for full-text search over step content
/// - `metadata` — arbitrary key/value annotations per run
pub struct SqliteProvider {
    conn: Connection,
}

impl SqliteProvider {
    /// Open (or create) the SQLite database at the default location for the current working
    /// directory (`~/.ail/projects/<sha1_of_cwd>/ail.db`).
    pub fn new() -> Result<Self, rusqlite::Error> {
        let path = project_dir().join("ail.db");
        Self::open(&path)
    }

    /// Open (or create) the SQLite database at an explicit path. Used in tests.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Self::create_schema(&conn)?;
        Self::migrate_schema(&conn)?;
        Ok(SqliteProvider { conn })
    }

    fn create_schema(conn: &Connection) -> SqliteResult<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                run_id          TEXT PRIMARY KEY,
                pipeline_source TEXT,
                started_at      INTEGER,
                completed_at    INTEGER,
                total_cost_usd  REAL,
                status          TEXT,
                project_hash    TEXT
            );

            CREATE TABLE IF NOT EXISTS steps (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id          TEXT NOT NULL REFERENCES sessions(run_id),
                step_id         TEXT NOT NULL,
                event_type      TEXT NOT NULL DEFAULT 'step_completed',
                prompt          TEXT,
                response        TEXT,
                cost_usd        REAL,
                input_tokens    INTEGER,
                output_tokens   INTEGER,
                thinking        TEXT,
                stdout          TEXT,
                stderr          TEXT,
                exit_code       INTEGER,
                model           TEXT,
                recorded_at     INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS traces (
                run_id  TEXT NOT NULL,
                step_id TEXT NOT NULL,
                content TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS traces_fts USING fts5(
                run_id UNINDEXED,
                step_id UNINDEXED,
                content,
                content='traces',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS traces_ai AFTER INSERT ON traces BEGIN
                INSERT INTO traces_fts(rowid, run_id, step_id, content)
                VALUES (new.rowid, new.run_id, new.step_id, new.content);
            END;

            CREATE TABLE IF NOT EXISTS metadata (
                run_id  TEXT NOT NULL REFERENCES sessions(run_id),
                key     TEXT NOT NULL,
                value   TEXT,
                PRIMARY KEY (run_id, key)
            );
            ",
        )
    }

    /// Migrate schema for existing databases. Adds missing columns if needed.
    fn migrate_schema(conn: &Connection) -> SqliteResult<()> {
        // Add project_hash column to sessions table if it doesn't exist
        let has_project_hash: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='project_hash'",
                [],
                |row| row.get::<_, u32>(0).map(|c| c > 0),
            )
            .unwrap_or(false);

        if !has_project_hash {
            conn.execute("ALTER TABLE sessions ADD COLUMN project_hash TEXT", [])?;
        }

        // Create run_events table if it doesn't exist.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS run_events (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id       TEXT NOT NULL,
                step_id      TEXT NOT NULL,
                seq          INTEGER NOT NULL,
                event_type   TEXT NOT NULL,
                tool_name    TEXT NOT NULL,
                tool_id      TEXT NOT NULL,
                content_json TEXT NOT NULL,
                recorded_at  TEXT NOT NULL
            )",
        )?;

        Ok(())
    }

    /// Ensure a session row exists (upsert). Operates on `conn` so it can be
    /// called inside a transaction.
    fn ensure_session_with(conn: &Connection, run_id: &str, value: &Value) -> SqliteResult<()> {
        // Extract pipeline_source from the value if present.
        let pipeline_source = value
            .get("pipeline_source")
            .or_else(|| value.get("source"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract project_hash from the value if present.
        let project_hash = value
            .get("project_hash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let now_ms = now_ms();

        conn.execute(
            "INSERT INTO sessions (run_id, pipeline_source, started_at, status, project_hash)
             VALUES (?1, ?2, ?3, 'running', ?4)
             ON CONFLICT(run_id) DO NOTHING",
            params![run_id, pipeline_source, now_ms, project_hash],
        )?;

        Ok(())
    }

    /// Insert a step log entry. Operates on `conn` so it can be called inside a transaction.
    ///
    /// Multiple SQL statements (steps, traces, session cost, run_events) are issued on
    /// the same connection; callers should wrap this in a transaction for crash safety.
    fn insert_step_with(conn: &Connection, run_id: &str, value: &Value) -> SqliteResult<()> {
        let step_id = value
            .get("step_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let event_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("step_completed");
        let prompt = value.get("prompt").and_then(|v| v.as_str());
        let response = value.get("response").and_then(|v| v.as_str());
        let cost_usd = value.get("cost_usd").and_then(|v| v.as_f64());
        let input_tokens = value.get("input_tokens").and_then(|v| v.as_i64());
        let output_tokens = value.get("output_tokens").and_then(|v| v.as_i64());
        let thinking = value.get("thinking").and_then(|v| v.as_str());
        let stdout = value.get("stdout").and_then(|v| v.as_str());
        let stderr = value.get("stderr").and_then(|v| v.as_str());
        let exit_code = value.get("exit_code").and_then(|v| v.as_i64());
        let model = value.get("model").and_then(|v| v.as_str());
        let now_ms = now_ms();

        conn.execute(
            "INSERT INTO steps
             (run_id, step_id, event_type, prompt, response, cost_usd,
              input_tokens, output_tokens, thinking, stdout, stderr,
              exit_code, model, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                run_id,
                step_id,
                event_type,
                prompt,
                response,
                cost_usd,
                input_tokens,
                output_tokens,
                thinking,
                stdout,
                stderr,
                exit_code,
                model,
                now_ms,
            ],
        )?;

        // Index searchable content in the traces table for FTS.
        let mut content_parts: Vec<&str> = Vec::new();
        if let Some(p) = prompt {
            content_parts.push(p);
        }
        if let Some(r) = response {
            content_parts.push(r);
        }
        if let Some(t) = thinking {
            content_parts.push(t);
        }
        if !content_parts.is_empty() {
            let content = content_parts.join("\n");
            conn.execute(
                "INSERT INTO traces (run_id, step_id, content) VALUES (?1, ?2, ?3)",
                params![run_id, step_id, content],
            )?;
        }

        // Update session cost accumulator and status.
        if event_type == "step_completed" || event_type == "step_failed" {
            conn.execute(
                "UPDATE sessions SET total_cost_usd = COALESCE(total_cost_usd, 0) + COALESCE(?1, 0)
                 WHERE run_id = ?2",
                params![cost_usd, run_id],
            )?;
        }

        // Insert tool events into run_events table.
        if let Some(tool_events) = value.get("tool_events").and_then(|v| v.as_array()) {
            let recorded_at = now_ms.to_string();
            for te in tool_events {
                let te_seq = te.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
                let te_event_type = te.get("event_type").and_then(|v| v.as_str()).unwrap_or("");
                let te_tool_name = te.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
                let te_tool_id = te.get("tool_id").and_then(|v| v.as_str()).unwrap_or("");
                let te_content_json = te
                    .get("content_json")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                conn.execute(
                    "INSERT INTO run_events
                     (run_id, step_id, seq, event_type, tool_name, tool_id, content_json, recorded_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        run_id,
                        step_id,
                        te_seq,
                        te_event_type,
                        te_tool_name,
                        te_tool_id,
                        te_content_json,
                        recorded_at,
                    ],
                )?;
            }
        }

        Ok(())
    }

    /// Mark the session as completed with the given status.
    pub fn finish_session(&self, run_id: &str, status: &str) -> SqliteResult<()> {
        let now_ms = now_ms();
        self.conn.execute(
            "UPDATE sessions SET status = ?1, completed_at = ?2 WHERE run_id = ?3",
            params![status, now_ms, run_id],
        )?;
        Ok(())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl LogProvider for SqliteProvider {
    fn write_entry(&mut self, run_id: &str, value: &Value) -> std::io::Result<()> {
        // Wrap ensure_session + insert_step (multiple SQL statements) in a single
        // transaction so a crash mid-write leaves the database in a consistent state.
        let tx = self.conn.transaction().map_err(std::io::Error::other)?;
        Self::ensure_session_with(&tx, run_id, value).map_err(std::io::Error::other)?;
        Self::insert_step_with(&tx, run_id, value).map_err(std::io::Error::other)?;
        tx.commit().map_err(std::io::Error::other)?;
        Ok(())
    }

    fn finish(&mut self, run_id: &str, status: &str) -> std::io::Result<()> {
        self.finish_session(run_id, status)
            .map_err(std::io::Error::other)
    }
}

/// Default db path for the current working directory.
pub fn db_path() -> PathBuf {
    project_dir().join("ail.db")
}
