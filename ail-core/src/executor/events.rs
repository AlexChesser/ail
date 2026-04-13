use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;

use crate::runner::{CancelToken, PermissionResponder, RunnerEvent};

/// Returned by `execute()` to distinguish successful completion variants.
#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ExecuteOutcome {
    /// All steps ran to completion.
    Completed,
    /// A `break` action fired; remaining steps were skipped. This is not an error.
    Break { step_id: String },
    /// An error occurred during execution.
    Error(String),
}

/// Signals the executor can receive from the TUI while a pipeline is running.
pub struct ExecutionControl {
    /// Set to `true` to request a pause between steps. The executor spin-waits until cleared.
    pub pause_requested: Arc<AtomicBool>,
    /// Cancellation token — call `cancel()` to request that the executor stop immediately
    /// after the current step and kill any in-flight runner subprocess.
    pub kill_requested: CancelToken,
    /// Callback for tool permission HITL via the MCP bridge (SPEC §13.3).
    /// Propagated into `InvokeOptions::permission_responder` for each runner invocation.
    pub permission_responder: Option<PermissionResponder>,
}

impl ExecutionControl {
    pub fn new() -> Self {
        ExecutionControl {
            pause_requested: Arc::new(AtomicBool::new(false)),
            kill_requested: CancelToken::new(),
            permission_responder: None,
        }
    }
}

impl Default for ExecutionControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_control_default_matches_new() {
        let c = ExecutionControl::default();
        assert!(!c.pause_requested.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!c.kill_requested.is_cancelled());
        assert!(c.permission_responder.is_none());
    }
}

/// Events emitted by `execute_with_control()` to the TUI.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutorEvent {
    StepStarted {
        step_id: String,
        step_index: usize,
        total_steps: usize,
        /// The resolved prompt that will be sent to the runner.
        /// `None` for non-prompt steps (context:shell, action, sub-pipeline).
        resolved_prompt: Option<String>,
    },
    StepCompleted {
        step_id: String,
        cost_usd: Option<f64>,
        input_tokens: u64,
        output_tokens: u64,
        /// The runner's response text.
        /// `None` for non-prompt steps (context:shell, action, sub-pipeline).
        response: Option<String>,
        /// Model used for this step, if available.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    StepSkipped {
        step_id: String,
    },
    StepFailed {
        step_id: String,
        error: String,
    },
    /// A step failed but `on_error: continue` is active — error logged, pipeline continues.
    StepErrorContinued {
        step_id: String,
        error: String,
        error_type: String,
    },
    /// A step failed but `on_error: retry` is active — retrying.
    StepRetrying {
        step_id: String,
        error: String,
        attempt: u32,
        max_retries: u32,
    },
    /// A `pause_for_human` step was reached — executor is blocked until `hitl_rx` receives a value.
    HitlGateReached {
        step_id: String,
        /// Optional operator-facing message from the step's `message:` YAML field.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// A `modify_output` HITL gate was reached (SPEC §13.2).
    /// The executor blocks until `hitl_rx` receives the modified text.
    HitlModifyReached {
        step_id: String,
        /// Optional operator-facing message from the step's `message:` YAML field.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// The last step response presented to the human for modification.
        #[serde(skip_serializing_if = "Option::is_none")]
        last_response: Option<String>,
    },
    /// A streaming event from the runner, nested under `event` so the inner `type` field is
    /// preserved in the NDJSON output. Using a named field avoids the internally-tagged
    /// newtype-of-tagged-enum serialization conflict that would overwrite the inner `type`.
    RunnerEvent {
        event: RunnerEvent,
    },
    /// Pipeline completed. The `outcome` field (`"completed"` or `"break"`) comes from
    /// `ExecuteOutcome`'s own `#[serde(tag = "outcome")]`, merged into this object by serde.
    PipelineCompleted(ExecuteOutcome),
    /// Pipeline aborted with an error.
    PipelineError {
        error: String,
        error_type: String,
    },
}
