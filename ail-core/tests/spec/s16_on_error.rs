//! SPEC §16 — `on_error` error handling with retry, continue, and abort_pipeline.

use ail_core::config::domain::{OnError, Pipeline, Step, StepBody, StepId};
use ail_core::error::{error_types, AilError};
use ail_core::executor::{execute, ExecuteOutcome};
use ail_core::runner::{InvokeOptions, RunResult, Runner};
use ail_core::session::log_provider::NullProvider;
use ail_core::session::Session;
use ail_core::test_helpers::{make_session, prompt_step};

use std::sync::atomic::{AtomicU32, Ordering};

// ── Helpers ─────────────────────────────────────────────────────────────────

/// A runner that fails the first N calls, then succeeds.
struct FailThenSucceedRunner {
    fail_count: AtomicU32,
    max_failures: u32,
    response: String,
}

impl FailThenSucceedRunner {
    fn new(max_failures: u32, response: &str) -> Self {
        FailThenSucceedRunner {
            fail_count: AtomicU32::new(0),
            max_failures,
            response: response.to_string(),
        }
    }

    fn invocation_count(&self) -> u32 {
        self.fail_count.load(Ordering::SeqCst)
    }
}

impl Runner for FailThenSucceedRunner {
    fn invoke(&self, _prompt: &str, _options: InvokeOptions) -> Result<RunResult, AilError> {
        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_failures {
            Err(AilError::runner_failed(format!(
                "simulated failure #{count}"
            )))
        } else {
            Ok(RunResult::stub(self.response.clone(), "stub-session-id"))
        }
    }
}

/// A runner that always fails.
struct AlwaysFailRunner;

impl Runner for AlwaysFailRunner {
    fn invoke(&self, _prompt: &str, _options: InvokeOptions) -> Result<RunResult, AilError> {
        Err(AilError::runner_failed("always fails"))
    }
}

fn step_with_on_error(id: &str, prompt: &str, on_error: Option<OnError>) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(prompt.to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        async_step: false,
        depends_on: vec![],
        on_error,
        before: vec![],
        then: vec![],
        output_schema: None,
        input_schema: None,
        sampling: None,
    }
}

// ── Default (abort) ─────────────────────────────────────────────────────────

/// §16 — Default behaviour (no on_error): step failure aborts pipeline.
#[test]
fn default_on_error_aborts_pipeline() {
    let step = step_with_on_error("failing", "hello", None);
    let mut session = make_session(vec![step]);
    let runner = AlwaysFailRunner;
    let result = execute(&mut session, &runner);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type(),
        error_types::RUNNER_INVOCATION_FAILED
    );
}

/// §16 — Explicit abort_pipeline: same as default.
#[test]
fn explicit_abort_pipeline_aborts_on_error() {
    let step = step_with_on_error("failing", "hello", Some(OnError::AbortPipeline));
    let mut session = make_session(vec![step]);
    let runner = AlwaysFailRunner;
    let result = execute(&mut session, &runner);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type(),
        error_types::RUNNER_INVOCATION_FAILED
    );
}

// ── Continue ────────────────────────────────────────────────────────────────

/// §16 — on_error: continue — log error and proceed to next step.
#[test]
fn on_error_continue_proceeds_to_next_step() {
    let failing = step_with_on_error("failing", "hello", Some(OnError::Continue));
    let ok = prompt_step("ok", "world");
    let mut session = make_session(vec![failing, ok]);
    let runner = FailThenSucceedRunner::new(1, "ok response");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
    // The failing step produces no turn entry (error was swallowed),
    // the second step produces one entry.
    let entries = session.turn_log.entries();
    assert_eq!(
        entries.len(),
        1,
        "only the 'ok' step should produce an entry"
    );
    assert_eq!(entries[0].step_id, "ok");
}

/// §16 — on_error: continue — pipeline completes even if the only step fails.
#[test]
fn on_error_continue_single_step_completes() {
    let failing = step_with_on_error("failing", "hello", Some(OnError::Continue));
    let mut session = make_session(vec![failing]);
    let runner = AlwaysFailRunner;
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
    assert!(
        session.turn_log.entries().is_empty(),
        "no entries for failed-and-continued step"
    );
}

// ── Retry ───────────────────────────────────────────────────────────────────

/// §16 — on_error: retry with max_retries: 2 — succeeds on 2nd attempt.
#[test]
fn on_error_retry_succeeds_within_max_retries() {
    let step = step_with_on_error("flaky", "hello", Some(OnError::Retry { max_retries: 3 }));
    let mut session = make_session(vec![step]);
    // Fails first attempt, succeeds on second.
    let runner = FailThenSucceedRunner::new(1, "retry succeeded");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), ExecuteOutcome::Completed));
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "flaky");
    assert_eq!(entries[0].response.as_deref(), Some("retry succeeded"));
    // Runner should have been called exactly 2 times (1 failure + 1 success).
    assert_eq!(runner.invocation_count(), 2);
}

/// §16 — on_error: retry — fails all retries, then aborts.
#[test]
fn on_error_retry_exhausts_retries_then_aborts() {
    let step = step_with_on_error("doomed", "hello", Some(OnError::Retry { max_retries: 2 }));
    let mut session = make_session(vec![step]);
    let runner = AlwaysFailRunner;
    let result = execute(&mut session, &runner);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type(),
        error_types::RUNNER_INVOCATION_FAILED,
        "should propagate the last error after retries exhausted"
    );
}

/// §16 — on_error: retry — runner is called exactly max_retries+1 times when always failing.
#[test]
fn on_error_retry_invokes_correct_number_of_times() {
    let step = step_with_on_error("counted", "hello", Some(OnError::Retry { max_retries: 3 }));
    let mut session = make_session(vec![step]);
    let runner = FailThenSucceedRunner::new(100, "never");
    let result = execute(&mut session, &runner);

    assert!(result.is_err());
    // 1 initial attempt + 3 retries = 4 total invocations.
    assert_eq!(runner.invocation_count(), 4);
}

/// §16 — on_error: retry — succeeds on the last possible retry.
#[test]
fn on_error_retry_succeeds_on_last_attempt() {
    let step = step_with_on_error(
        "last_chance",
        "hello",
        Some(OnError::Retry { max_retries: 2 }),
    );
    let mut session = make_session(vec![step]);
    // Fails first 2 attempts, succeeds on 3rd (max_retries+1).
    let runner = FailThenSucceedRunner::new(2, "just in time");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].response.as_deref(), Some("just in time"));
    assert_eq!(runner.invocation_count(), 3);
}

// ── Retry followed by more steps ────────────────────────────────────────────

/// §16 — after a successful retry, subsequent steps run normally.
#[test]
fn retry_success_then_next_step_runs() {
    let flaky = step_with_on_error("flaky", "hello", Some(OnError::Retry { max_retries: 2 }));
    let after = prompt_step("after", "world");
    let mut session = make_session(vec![flaky, after]);
    let runner = FailThenSucceedRunner::new(1, "recovered");
    let result = execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].step_id, "flaky");
    assert_eq!(entries[1].step_id, "after");
}

// ── Validation ──────────────────────────────────────────────────────────────

/// §16 — on_error: continue round-trips through YAML loading.
#[test]
fn on_error_continue_round_trips_through_yaml() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"1\"\npipeline:\n  - id: s1\n    prompt: hello\n    on_error: continue\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let pipeline = ail_core::config::load(&path).expect("should load");
    assert_eq!(
        pipeline.steps[0].on_error,
        Some(OnError::Continue),
        "on_error should be Continue"
    );
}

/// §16 — on_error: retry with max_retries round-trips through YAML loading.
#[test]
fn on_error_retry_round_trips_through_yaml() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"1\"\npipeline:\n  - id: s1\n    prompt: hello\n    on_error: retry\n    max_retries: 3\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let pipeline = ail_core::config::load(&path).expect("should load");
    assert_eq!(
        pipeline.steps[0].on_error,
        Some(OnError::Retry { max_retries: 3 }),
        "on_error should be Retry with max_retries 3"
    );
}

/// §16 — on_error: retry without max_retries fails validation.
#[test]
fn on_error_retry_without_max_retries_fails_validation() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"1\"\npipeline:\n  - id: s1\n    prompt: hello\n    on_error: retry\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let result = ail_core::config::load(&path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    assert!(
        err.detail().contains("max_retries"),
        "error should mention max_retries, got: {}",
        err.detail()
    );
}

/// §16 — max_retries without on_error fails validation.
#[test]
fn max_retries_without_on_error_fails_validation() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"1\"\npipeline:\n  - id: s1\n    prompt: hello\n    max_retries: 3\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let result = ail_core::config::load(&path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
}

/// §16 — unknown on_error value fails validation.
#[test]
fn unknown_on_error_value_fails_validation() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"1\"\npipeline:\n  - id: s1\n    prompt: hello\n    on_error: panic\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let result = ail_core::config::load(&path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    assert!(
        err.detail().contains("panic"),
        "error should mention the unknown value, got: {}",
        err.detail()
    );
}

/// §16 — max_retries: 0 with on_error: retry fails validation.
#[test]
fn max_retries_zero_fails_validation() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut file = NamedTempFile::new().expect("tempfile");
    writeln!(
        file,
        "version: \"1\"\npipeline:\n  - id: s1\n    prompt: hello\n    on_error: retry\n    max_retries: 0\n"
    )
    .expect("write");
    let path = file.path().to_path_buf();

    let result = ail_core::config::load(&path);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    assert!(
        err.detail().contains("max_retries"),
        "error should mention max_retries, got: {}",
        err.detail()
    );
}
