//! Shared test helpers used across in-crate and integration tests.
//!
//! This module is `#[doc(hidden)]` and intended for test use only.
//! It is always compiled (not gated by `cfg(test)`) so that integration tests
//! in `tests/` can import it via `use ail_core::test_helpers::*;`.
//! In-crate tests use `use crate::test_helpers::*;`.

use crate::config::domain::{Pipeline, Step, StepBody, StepId};
use crate::session::log_provider::NullProvider;
use crate::session::Session;

impl Default for Step {
    fn default() -> Self {
        Step {
            id: StepId(String::new()),
            body: StepBody::Prompt(String::new()),
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: false,
            on_error: None,
            before: vec![],
            then: vec![],
        }
    }
}

/// Creates a [`Pipeline`] with the given steps and all other fields defaulted.
pub fn make_pipeline(steps: Vec<Step>) -> Pipeline {
    Pipeline { steps, ..Default::default() }
}

/// Creates a [`Session`] backed by a [`NullProvider`] with the given pipeline steps.
pub fn make_session(steps: Vec<Step>) -> Session {
    Session::new(make_pipeline(steps), "invocation prompt".to_string())
        .with_log_provider(Box::new(NullProvider))
}

/// Creates a [`Step`] with a [`StepBody::Prompt`] body and all other fields set to
/// `None`/`false`/default.
pub fn prompt_step(id: &str, text: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
        ..Default::default()
    }
}
