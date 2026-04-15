//! Skill step dispatch — resolve skill name, generate prompt, invoke runner, return TurnEntry.

#![allow(clippy::result_large_err)]

use crate::config::domain::Step;
use crate::error::AilError;
use crate::runner::{InvokeOptions, Runner};
use crate::session::{Session, TurnEntry};
use crate::skill::SkillRegistry;
use crate::template;

use crate::executor::core::StepObserver;
use crate::executor::helpers::{
    build_step_runner_box, build_tool_policy, resolve_effective_runner_name, resolve_step_provider,
    resolve_step_sampling, resolve_step_system_prompts,
};

/// Execute a skill step: resolve skill name, build prompt from template, invoke runner.
#[allow(clippy::too_many_arguments)]
pub(in crate::executor) fn execute<O: StepObserver>(
    skill_name: &str,
    step: &Step,
    session: &mut Session,
    runner: &(dyn Runner + Sync),
    step_id: &str,
    step_index: usize,
    total_steps: usize,
    pipeline_base_dir: Option<&std::path::Path>,
    observer: &mut O,
) -> Result<TurnEntry, AilError> {
    // Look up the skill in the built-in registry.
    let registry = SkillRegistry::new();
    let skill_def = registry.resolve(skill_name).map_err(|e| {
        let err = e.with_step_context(&session.run_id, step_id);
        observer.on_step_failed(step_id, err.detail());
        err
    })?;

    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        skill = %skill_name,
        "resolving skill prompt template"
    );

    // Resolve template variables in the skill's prompt template.
    let resolved = template::resolve(&skill_def.prompt_template, session)
        .map_err(|e| e.with_step_context(&session.run_id, step_id))
        .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;

    observer.on_prompt_ready(step_id, step_index, total_steps, &resolved);

    let resume_id = if step.resume {
        session
            .turn_log
            .last_runner_session_id()
            .map(|s| s.to_string())
    } else {
        None
    };
    session.turn_log.record_step_started(step_id, &resolved);

    let (resolved_system_prompt, resolved_append_system_prompt) =
        resolve_step_system_prompts(step, session, step_id, pipeline_base_dir)
            .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;

    let resolved_provider = resolve_step_provider(session, step);
    let effective_tools = step
        .tools
        .as_ref()
        .or(session.pipeline.default_tools.as_ref());
    session.runner_name = resolve_effective_runner_name(step);
    let step_runner_box = build_step_runner_box(
        step,
        session.headless,
        &session.http_session_store,
        &resolved_provider,
    )
    .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;
    let effective_runner: &dyn Runner = step_runner_box
        .as_deref()
        .map(|b| b as &dyn Runner)
        .unwrap_or(runner);

    let extensions = effective_runner.build_extensions(&resolved_provider);
    let sampling = resolve_step_sampling(session, step);
    let mut options = InvokeOptions {
        resume_session_id: resume_id,
        tool_policy: build_tool_policy(effective_tools),
        model: resolved_provider.model,
        extensions,
        permission_responder: None,
        cancel_token: None,
        system_prompt: resolved_system_prompt,
        append_system_prompt: resolved_append_system_prompt,
        sampling: sampling.clone(),
    };
    observer.augment_options(&mut options);

    let result = observer
        .invoke(effective_runner, &resolved, options)
        .inspect_err(|e| observer.on_step_failed(step_id, e.detail()))?;

    tracing::info!(
        run_id = %session.run_id,
        step_id = %step_id,
        skill = %skill_name,
        cost_usd = ?result.cost_usd,
        "skill step complete"
    );
    observer.on_prompt_completed(step_id, &result);

    Ok(
        TurnEntry::from_prompt(step_id.to_string(), resolved, result)
            .with_sampling(sampling.as_ref()),
    )
}
