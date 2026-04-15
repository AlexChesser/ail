use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::config::domain::{Pipeline, ProviderConfig};
use crate::runner::http::HttpSessionStore;

use super::log_provider::{CompositeProvider, JsonlProvider, LogProvider};
use super::sqlite_provider::SqliteProvider;
use super::turn_log::TurnLog;

/// Active do_while loop context, set during loop body execution.
/// Enables `{{ do_while.iteration }}` and `{{ do_while.max_iterations }}` template
/// variables, and provides the scope prefix for namespaced step ID resolution.
#[derive(Debug, Clone)]
pub struct DoWhileContext {
    /// The do_while step's ID (used as the `<loop_id>::` namespace prefix).
    pub loop_id: String,
    /// Current 0-based iteration index.
    pub iteration: u64,
    /// The declared `max_iterations` value.
    pub max_iterations: u64,
}

/// Active for_each loop context, set during loop body execution (SPEC §28).
/// Enables `{{ for_each.item }}` / `{{ for_each.<as_name> }}`, `{{ for_each.index }}`,
/// and `{{ for_each.total }}` template variables.
#[derive(Debug, Clone)]
pub struct ForEachContext {
    /// The for_each step's ID (used as the `<loop_id>::` namespace prefix).
    pub loop_id: String,
    /// Current 1-based item index.
    pub index: u64,
    /// Total number of items (after max_items cap).
    pub total: u64,
    /// The current item value as a JSON string.
    pub item: String,
    /// The declared `as` name (default: `item`).
    pub as_name: String,
}

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
    /// Shared HTTP runner session store — all HttpRunner instances in this pipeline run
    /// share the same in-memory conversation map.
    pub http_session_store: HttpSessionStore,
    /// Active do_while loop context (SPEC §27). Set during loop body execution,
    /// cleared after the loop exits. Enables `{{ do_while.* }}` template variables
    /// and namespaced step ID resolution.
    pub do_while_context: Option<DoWhileContext>,
    /// Active for_each loop context (SPEC §28). Set during loop body execution,
    /// cleared after the loop exits. Enables `{{ for_each.* }}` template variables
    /// and namespaced step ID resolution.
    pub for_each_context: Option<ForEachContext>,
    /// Current nesting depth of loop constructs (do_while, for_each). Checked against
    /// `MAX_LOOP_DEPTH` to prevent runaway resource consumption from deeply nested loops.
    pub loop_depth: usize,
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
            http_session_store: Arc::new(Mutex::new(HashMap::new())),
            do_while_context: None,
            for_each_context: None,
            loop_depth: 0,
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

    /// Fork a branch session for parallel async step execution (SPEC §29.9).
    ///
    /// The branch gets:
    /// - A fresh `TurnLog` seeded with a clone of the parent's existing entries
    ///   (so template resolution of prior steps works inside the branch).
    /// - `NullProvider` for log persistence — branch results are collected and
    ///   merged back into the main session's turn log after the join barrier.
    /// - The same `run_id`, `invocation_prompt`, `cwd`, `runner_name`, `headless`,
    ///   `cli_provider` as the parent.
    /// - A fresh `http_session_store` when `isolated_http` is true (for
    ///   `resume: false` async steps that opt out of context inheritance).
    ///   Otherwise shares the parent's store.
    /// - Cleared loop contexts — loop bodies spawning async steps is an
    ///   unsupported edge case the spec defers; branches run sequentially inside.
    pub fn fork_for_branch(&self, isolated_http: bool) -> Session {
        let entries: Vec<super::turn_log::TurnEntry> = self.turn_log.entries().to_vec();

        let mut turn_log = TurnLog::with_provider(
            self.run_id.clone(),
            Box::new(super::log_provider::NullProvider),
        );
        for e in entries {
            turn_log.append(e);
        }

        let http_session_store = if isolated_http {
            Arc::new(Mutex::new(HashMap::new()))
        } else {
            self.http_session_store.clone()
        };

        Session {
            run_id: self.run_id.clone(),
            pipeline: self.pipeline.clone(),
            invocation_prompt: self.invocation_prompt.clone(),
            turn_log,
            cli_provider: self.cli_provider.clone(),
            cwd: self.cwd.clone(),
            runner_name: self.runner_name.clone(),
            headless: self.headless,
            http_session_store,
            do_while_context: self.do_while_context.clone(),
            for_each_context: self.for_each_context.clone(),
            loop_depth: self.loop_depth,
        }
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
            ..Default::default()
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
