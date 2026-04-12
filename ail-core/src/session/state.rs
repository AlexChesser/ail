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
    /// CLI-supplied provider/model overrides. Highest priority in the resolution chain:
    /// pipeline defaults → per-step model → cli_provider.
    pub cli_provider: ProviderConfig,
    /// Working directory captured at session creation time (used by `{{ session.cwd }}`).
    pub cwd: String,
    /// The name of the runner currently active for the executing step.
    /// Updated by the executor before each Prompt step so `{{ session.tool }}` reflects
    /// the resolved runner (per-step `runner:` override → `AIL_DEFAULT_RUNNER` → `"claude"`).
    pub runner_name: String,
    /// Whether this session runs in headless mode (`--dangerously-skip-permissions`).
    /// Propagated to per-step runner overrides via `build_step_runner_box`.
    pub headless: bool,
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

        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        let runner_name =
            std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string());

        Session {
            run_id,
            pipeline,
            invocation_prompt,
            turn_log,
            cli_provider: ProviderConfig::default(),
            cwd,
            runner_name,
            headless: false,
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

    /// Returns `true` when the first pipeline step has id `"invocation"`, meaning the
    /// pipeline owns the invocation step and the host must not run it separately.
    pub fn has_invocation_step(&self) -> bool {
        self.pipeline
            .steps
            .first()
            .map(|s| s.id.as_str() == "invocation")
            .unwrap_or(false)
    }

    /// Set the pipeline source name for this session (useful in tests and sub-pipeline contexts).
    /// Re-emits `run_started` so the turn log provider records the correct source.
    pub fn with_pipeline(mut self, name: &str) -> Self {
        self.pipeline.source = Some(std::path::PathBuf::from(name));
        let pipeline_source = self.pipeline.source.as_deref().and_then(|p| p.to_str());
        self.turn_log.record_run_started(pipeline_source);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::Pipeline;
    use crate::session::log_provider::NullProvider;

    fn make_session() -> Session {
        Session::new(Pipeline::passthrough(), "test prompt".to_string())
            .with_log_provider(Box::new(NullProvider))
    }

    #[test]
    fn session_new_run_id_is_nonempty_uuid_string() {
        let session = make_session();
        assert!(!session.run_id.is_empty());
        // UUIDs are 36 chars: 8-4-4-4-12 with dashes
        assert_eq!(session.run_id.len(), 36, "run_id should be a UUID string");
        assert!(session.run_id.contains('-'), "run_id should contain dashes");
    }

    #[test]
    fn two_session_new_calls_produce_distinct_run_ids() {
        let s1 = make_session();
        let s2 = make_session();
        assert_ne!(s1.run_id, s2.run_id);
    }

    #[test]
    fn with_log_provider_chaining_works() {
        let session = Session::new(Pipeline::passthrough(), "hello".to_string())
            .with_log_provider(Box::new(NullProvider));
        // If chaining works, the session is usable and has the expected prompt
        assert_eq!(session.invocation_prompt, "hello");
        assert!(session.turn_log.entries().is_empty());
    }

    #[test]
    fn with_pipeline_sets_source_on_pipeline() {
        let session = make_session().with_pipeline("my-pipeline.ail.yaml");
        let source = session
            .pipeline
            .source
            .as_ref()
            .expect("source should be set");
        assert_eq!(source.to_str().unwrap(), "my-pipeline.ail.yaml");
    }

    #[test]
    fn invocation_prompt_equals_prompt_passed_to_new() {
        let session = make_session();
        assert_eq!(session.invocation_prompt, "test prompt");
    }

    #[test]
    fn has_invocation_step_returns_true_for_passthrough() {
        // Pipeline::passthrough() declares "invocation" as its first step.
        let session = make_session();
        assert!(session.has_invocation_step());
    }

    #[test]
    fn has_invocation_step_returns_true_when_first_step_is_invocation() {
        use crate::config::domain::{Step, StepBody, StepId};
        use crate::test_helpers::make_session as helpers_make_session;
        let step = Step {
            id: StepId("invocation".to_string()),
            body: StepBody::Prompt("hello".to_string()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
        };
        let session = helpers_make_session(vec![step]);
        assert!(session.has_invocation_step());
    }

    #[test]
    fn has_invocation_step_returns_false_when_first_step_is_not_invocation() {
        use crate::test_helpers::{make_session as helpers_make_session, prompt_step};
        let session = helpers_make_session(vec![prompt_step("other", "text")]);
        assert!(!session.has_invocation_step());
    }
}
