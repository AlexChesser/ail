use uuid::Uuid;

use crate::config::domain::{Pipeline, ProviderConfig};

use super::log_provider::{CompositeProvider, JsonlProvider, LogProvider};
use super::sqlite_provider::SqliteProvider;
use super::turn_log::TurnLog;

pub struct Session {
    pub run_id: String,
    pub pipeline: Pipeline,
    pub invocation_prompt: String,
    pub turn_log: TurnLog,
    pub tool_allowlist: Vec<String>,
    /// CLI-supplied provider/model overrides. Highest priority in the resolution chain:
    /// pipeline defaults → per-step model → cli_provider.
    pub cli_provider: ProviderConfig,
}

impl Session {
    pub fn new(pipeline: Pipeline, invocation_prompt: String) -> Self {
        let run_id = Uuid::new_v4().to_string();

        // Build a composite provider: always write JSONL; also write SQLite when available.
        // SQLite failure is non-fatal — fall back to JSONL-only.
        let providers: Vec<Box<dyn LogProvider>> = {
            let mut v: Vec<Box<dyn LogProvider>> = vec![Box::new(JsonlProvider::new())];
            match SqliteProvider::new() {
                Ok(sqlite) => v.push(Box::new(sqlite)),
                Err(e) => {
                    tracing::warn!(error = %e, "sqlite provider unavailable, using jsonl only")
                }
            }
            v
        };

        let mut turn_log =
            TurnLog::with_provider(run_id.clone(), Box::new(CompositeProvider::new(providers)));

        let pipeline_source = pipeline.source.as_deref().and_then(|p| p.to_str());
        turn_log.record_run_started(pipeline_source);

        Session {
            run_id,
            pipeline,
            invocation_prompt,
            turn_log,
            tool_allowlist: Vec::new(),
            cli_provider: ProviderConfig::default(),
        }
    }

    /// Replace the default provider with a custom `LogProvider`. Useful for tests.
    /// Re-emits `run_started` so the new provider receives the pipeline_source entry.
    pub fn with_log_provider(mut self, provider: Box<dyn LogProvider>) -> Self {
        let pipeline_source = self.pipeline.source.as_deref().and_then(|p| p.to_str());
        self.turn_log = TurnLog::with_provider(self.run_id.clone(), provider);
        self.turn_log.record_run_started(pipeline_source);
        self
    }
}
