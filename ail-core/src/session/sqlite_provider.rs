use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde_json::Value;

use super::log_provider::project_dir;

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// SQLite-backed persistence provider with FTS5 full-text search.
///
/// Database lives at `~/.ail/projects/<sha1_of_cwd>/ail.db` — same project
/// directory as the JSONL files.
pub struct SqliteProvider {
    conn: Connection,
}

impl SqliteProvider {
    /// Open (or create) the default database for the current working directory.
    pub fn new(run_id: String) -> Result<Self, rusqlite::Error> {
        let db_path = project_dir().join("ail.db");
        let _ = run_id; // run_id passed per write_entry call; not stored on struct
        Self::open(db_path)
    }

    /// Open (or create) a database at an explicit path. Primarily for testing.
    pub fn open(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        // Ensure the parent directory exists (best-effort, same as JsonlProvider).
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Self::migrate(&conn)?;
        Ok(SqliteProvider { conn })
    }

    fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if version < 1 {
            conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS sessions (
                    run_id          TEXT PRIMARY KEY,
                    pipeline_source TEXT,
                    started_at      INTEGER,
                    completed_at    INTEGER,
                    total_cost_usd  REAL,
                    status          TEXT
                );

                CREATE TABLE IF NOT EXISTS steps (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    run_id        TEXT    NOT NULL REFERENCES sessions(run_id),
                    step_id       TEXT    NOT NULL,
                    event_type    TEXT    NOT NULL,
                    prompt        TEXT,
                    response      TEXT,
                    cost_usd      REAL,
                    input_tokens  INTEGER,
                    output_tokens INTEGER,
                    thinking      TEXT,
                    stdout        TEXT,
                    stderr        TEXT,
                    exit_code     INTEGER,
                    model         TEXT,
                    recorded_at   INTEGER NOT NULL
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS traces USING fts5(
                    run_id   UNINDEXED,
                    step_id  UNINDEXED,
                    content,
                    tokenize='porter unicode61'
                );

                CREATE TABLE IF NOT EXISTS metadata (
                    run_id TEXT NOT NULL REFERENCES sessions(run_id),
                    key    TEXT NOT NULL,
                    value  TEXT,
                    PRIMARY KEY (run_id, key)
                );

                PRAGMA user_version = 1;
                ",
            )?;
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    fn ensure_session(&self, run_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (run_id, started_at, status)
             VALUES (?1, ?2, 'running')",
            rusqlite::params![run_id, now_millis()],
        )?;
        Ok(())
    }

    fn handle_step_started(&self, run_id: &str, value: &Value) -> rusqlite::Result<()> {
        self.ensure_session(run_id)?;

        let step_id = value["step_id"].as_str().unwrap_or("");
        let prompt = value["prompt"].as_str();

        self.conn.execute(
            "INSERT INTO steps
                (run_id, step_id, event_type, prompt, recorded_at)
             VALUES (?1, ?2, 'step_started', ?3, ?4)",
            rusqlite::params![run_id, step_id, prompt, now_millis()],
        )?;
        Ok(())
    }

    fn handle_turn_entry(&self, run_id: &str, value: &Value) -> rusqlite::Result<()> {
        self.ensure_session(run_id)?;

        let step_id = value["step_id"].as_str().unwrap_or("");
        let prompt = value["prompt"].as_str();
        let response = value["response"].as_str();
        let cost_usd = value["cost_usd"].as_f64();
        let input_tokens = value["input_tokens"].as_i64();
        let output_tokens = value["output_tokens"].as_i64();
        let thinking = value["thinking"].as_str();
        let stdout = value["stdout"].as_str();
        let stderr = value["stderr"].as_str();
        let exit_code = value["exit_code"].as_i64();

        self.conn.execute(
            "INSERT INTO steps
                (run_id, step_id, event_type, prompt, response, cost_usd,
                 input_tokens, output_tokens, thinking, stdout, stderr,
                 exit_code, recorded_at)
             VALUES (?1, ?2, 'turn_entry', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                run_id,
                step_id,
                prompt,
                response,
                cost_usd,
                input_tokens,
                output_tokens,
                thinking,
                stdout,
                stderr,
                exit_code,
                now_millis(),
            ],
        )?;

        // Accumulate cost into session total.
        if let Some(cost) = cost_usd {
            self.conn.execute(
                "UPDATE sessions
                 SET total_cost_usd = COALESCE(total_cost_usd, 0.0) + ?1
                 WHERE run_id = ?2",
                rusqlite::params![cost, run_id],
            )?;
        }

        // FTS index: any non-null searchable content fields.
        for content in [response, thinking, stdout, stderr].into_iter().flatten() {
            if !content.is_empty() {
                self.conn.execute(
                    "INSERT INTO traces (run_id, step_id, content) VALUES (?1, ?2, ?3)",
                    rusqlite::params![run_id, step_id, content],
                )?;
            }
        }

        Ok(())
    }

    fn handle_pipeline_terminal(&self, run_id: &str, status: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET completed_at = ?1, status = ?2 WHERE run_id = ?3",
            rusqlite::params![now_millis(), status, run_id],
        )?;
        Ok(())
    }
}

impl super::log_provider::LogProvider for SqliteProvider {
    fn write_entry(&mut self, run_id: &str, value: &Value) -> std::io::Result<()> {
        let result = match value["type"].as_str() {
            Some("step_started") => self.handle_step_started(run_id, value),
            Some("pipeline_completed") => self.handle_pipeline_terminal(run_id, "completed"),
            Some("pipeline_error") => self.handle_pipeline_terminal(run_id, "failed"),
            _ if value.get("step_id").is_some() => self.handle_turn_entry(run_id, value),
            _ => {
                tracing::warn!(
                    run_id = %run_id,
                    "sqlite_provider: unrecognised event shape, skipping"
                );
                Ok(())
            }
        };

        result.map_err(std::io::Error::other)
    }
}
