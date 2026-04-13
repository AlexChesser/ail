//! Sub-pipeline dispatch — recursion, depth guard, child session creation.
//! Also handles named pipeline execution (SPEC §10).

#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::time::SystemTime;

use crate::config::domain::{Pipeline, MAX_SUB_PIPELINE_DEPTH};
use crate::error::AilError;
use crate::runner::Runner;
use crate::session::{Session, TurnEntry};
use crate::template;

use crate::executor::core::{execute_core, NullObserver, StepObserver};

/// Load and execute a sub-pipeline, returning a `TurnEntry` for the calling step.
///
/// The `path_template` may contain `{{ variable }}` syntax (SPEC §11); it is resolved
/// against `session` before the file is loaded. The sub-pipeline runs in isolation:
/// a fresh `Session` is created with the parent's `last_response` as its invocation prompt.
/// The child's final step response becomes the returned entry's `response` field.
///
/// `depth` guards against infinite recursion; exceeding `MAX_SUB_PIPELINE_DEPTH` aborts.
///
/// **Note:** sub-pipelines always execute in headless mode, even when the parent pipeline
/// is running in controlled mode. Sub-pipeline streaming events and HITL gates are not
/// propagated to the parent's TUI. This is a known limitation (v0.2).
#[allow(clippy::too_many_arguments)]
pub(in crate::executor) fn execute<O: StepObserver>(
    path_template: &str,
    prompt_override: Option<&str>,
    step_id: &str,
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
    base_dir: Option<&std::path::Path>,
    observer: &mut O,
) -> Result<TurnEntry, AilError> {
    session.turn_log.record_step_started(step_id, path_template);
    let entry = execute_sub_pipeline(
        path_template,
        prompt_override,
        step_id,
        session,
        runner,
        depth,
        base_dir,
    )
    .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;
    observer.on_non_prompt_completed(step_id);
    Ok(entry)
}

/// Inner sub-pipeline execution logic. Also used by the on_result pipeline action
/// in `core.rs`.
pub(in crate::executor) fn execute_sub_pipeline(
    path_template: &str,
    prompt_override: Option<&str>,
    step_id: &str,
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
    base_dir: Option<&std::path::Path>,
) -> Result<TurnEntry, AilError> {
    if depth >= MAX_SUB_PIPELINE_DEPTH {
        return Err(AilError::PipelineAborted {
            detail: format!(
                "Step '{step_id}' would exceed the maximum sub-pipeline nesting depth \
                 of {MAX_SUB_PIPELINE_DEPTH}"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                step_id,
            )),
        });
    }

    // Resolve template variables in the path (SPEC §11).
    let resolved_path = template::resolve(path_template, session)
        .map_err(|e| e.with_step_context(&session.run_id, step_id))?;

    // Resolve ./relative and ../relative paths against the parent pipeline's directory (SPEC §9).
    let path_buf = if let (true, Some(base)) = (
        resolved_path.starts_with("./") || resolved_path.starts_with("../"),
        base_dir,
    ) {
        base.join(&resolved_path)
    } else {
        std::path::PathBuf::from(&resolved_path)
    };

    let sub_pipeline = crate::config::load(path_buf.as_path())
        .map_err(|e| e.with_step_context(&session.run_id, step_id))?;

    // The sub-pipeline's invocation prompt: use the explicit override when provided
    // (template-resolved against the parent session), otherwise fall back to the
    // parent's most recent response (SPEC §9).
    let invocation_prompt = if let Some(override_template) = prompt_override {
        template::resolve(override_template, session)
            .map_err(|e| e.with_step_context(&session.run_id, step_id))?
    } else {
        session
            .turn_log
            .last_response()
            .unwrap_or(&session.invocation_prompt)
            .to_string()
    };

    let mut child_session = crate::session::Session::new(sub_pipeline, invocation_prompt);
    child_session.cli_provider = session.cli_provider.clone();

    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        sub_pipeline = %resolved_path,
        depth,
        "executing sub-pipeline"
    );

    execute_core(&mut child_session, runner, &mut NullObserver, depth + 1)?;

    let response = child_session
        .turn_log
        .last_response()
        .unwrap_or("")
        .to_string();

    Ok(TurnEntry {
        step_id: step_id.to_string(),
        prompt: resolved_path,
        response: Some(response),
        timestamp: SystemTime::now(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        runner_session_id: child_session
            .turn_log
            .last_runner_session_id()
            .map(str::to_string),
        stdout: None,
        stderr: None,
        exit_code: None,
        thinking: None,
        tool_events: vec![],
        modified: None,
        index: None,
    })
}

// ── Named pipeline execution (SPEC §10) ─────────────────────────────────────

/// Execute a named pipeline reference, returning a `TurnEntry` for the calling step.
///
/// Named pipelines are defined in the `pipelines:` section of the YAML file and
/// referenced by name from `pipeline:` step bodies. The named pipeline's steps
/// are cloned into a child `Pipeline` and executed in isolation (fresh `Session`).
///
/// Circular references are detected by tracking visited pipeline names through
/// the call chain. If a name is encountered again, a typed
/// `PIPELINE_CIRCULAR_REFERENCE` error is returned.
#[allow(clippy::too_many_arguments)]
pub(in crate::executor) fn execute_named<O: StepObserver>(
    name: &str,
    prompt_override: Option<&str>,
    step_id: &str,
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
    observer: &mut O,
) -> Result<TurnEntry, AilError> {
    session.turn_log.record_step_started(step_id, name);
    let entry = execute_named_pipeline(
        name,
        prompt_override,
        step_id,
        session,
        runner,
        depth,
        &mut HashSet::new(),
    )
    .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;
    observer.on_non_prompt_completed(step_id);
    Ok(entry)
}

/// Inner named pipeline execution logic.
///
/// `visited` tracks the set of named pipeline names already in the call chain
/// to detect circular references. Each recursive call adds the current name
/// before executing, and the caller is responsible for inserting the root name.
pub(in crate::executor) fn execute_named_pipeline(
    name: &str,
    prompt_override: Option<&str>,
    step_id: &str,
    session: &mut Session,
    runner: &dyn Runner,
    depth: usize,
    visited: &mut HashSet<String>,
) -> Result<TurnEntry, AilError> {
    if depth >= MAX_SUB_PIPELINE_DEPTH {
        return Err(AilError::PipelineAborted {
            detail: format!(
                "Step '{step_id}' would exceed the maximum sub-pipeline nesting depth \
                 of {MAX_SUB_PIPELINE_DEPTH}"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                step_id,
            )),
        });
    }

    // Circular reference detection
    if !visited.insert(name.to_string()) {
        return Err(AilError::PipelineCircularReference {
            detail: format!(
                "Circular reference detected: named pipeline '{name}' references itself \
                 (directly or transitively)"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                step_id,
            )),
        });
    }

    // Look up the named pipeline in the parent pipeline's definitions.
    let named_steps = session
        .pipeline
        .named_pipelines
        .get(name)
        .ok_or_else(|| AilError::PipelineAborted {
            detail: format!(
                "Step '{step_id}' references named pipeline '{name}' which is not defined \
                 in the pipelines: section"
            ),
            context: Some(crate::error::ErrorContext::for_step(
                &session.run_id,
                step_id,
            )),
        })?
        .clone();

    // The child session's invocation prompt: use the explicit override when provided
    // (template-resolved against the parent session), otherwise fall back to the
    // parent's most recent response (SPEC §10, §9).
    let invocation_prompt = if let Some(override_template) = prompt_override {
        template::resolve(override_template, session)
            .map_err(|e| e.with_step_context(&session.run_id, step_id))?
    } else {
        session
            .turn_log
            .last_response()
            .unwrap_or(&session.invocation_prompt)
            .to_string()
    };

    // Build a child pipeline from the named pipeline's steps. Named pipelines
    // from the parent are passed through so nested named pipeline references work.
    let child_pipeline = Pipeline {
        steps: named_steps,
        source: session.pipeline.source.clone(),
        defaults: session.pipeline.defaults.clone(),
        timeout_seconds: session.pipeline.timeout_seconds,
        default_tools: session.pipeline.default_tools.clone(),
        named_pipelines: session.pipeline.named_pipelines.clone(),
    };

    let mut child_session = crate::session::Session::new(child_pipeline, invocation_prompt);
    child_session.cli_provider = session.cli_provider.clone();

    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        named_pipeline = %name,
        depth,
        "executing named pipeline"
    );

    execute_core(&mut child_session, runner, &mut NullObserver, depth + 1)?;

    let response = child_session
        .turn_log
        .last_response()
        .unwrap_or("")
        .to_string();

    Ok(TurnEntry {
        step_id: step_id.to_string(),
        prompt: format!("named:{name}"),
        response: Some(response),
        timestamp: SystemTime::now(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        runner_session_id: child_session
            .turn_log
            .last_runner_session_id()
            .map(str::to_string),
        stdout: None,
        stderr: None,
        exit_code: None,
        thinking: None,
        tool_events: vec![],
        modified: None,
        index: None,
    })
}
