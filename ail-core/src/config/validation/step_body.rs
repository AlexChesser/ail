//! Step body parsing — primary field selection and body construction.

#![allow(clippy::result_large_err)]

use crate::config::domain::{ActionKind, ContextSource, HitlHeadlessBehavior, StepBody};
use crate::config::dto::StepDto;
use crate::error::AilError;

use super::cfg_err;

/// Parse the step body from a DTO, enforcing exactly one primary field.
pub(in crate::config) fn parse_step_body(
    step_dto: &StepDto,
    id_str: &str,
) -> Result<StepBody, AilError> {
    // When pipeline: is set, prompt: is treated as the child invocation override,
    // not a primary field — so don't count it in the primary field selector.
    let primary_count = [
        step_dto.prompt.is_some() && step_dto.pipeline.is_none(),
        step_dto.skill.is_some(),
        step_dto.pipeline.is_some(),
        step_dto.action.is_some(),
        step_dto.context.is_some(),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if primary_count != 1 {
        return Err(cfg_err!(
            "Step '{id_str}' must have exactly one primary field \
             (prompt, skill, pipeline, action, or context); found {primary_count}"
        ));
    }

    // pipeline: is checked before prompt: so that pipeline+prompt correctly creates a
    // SubPipeline step with prompt as the child invocation override (SPEC §9.3).
    if let Some(ref pipeline_path) = step_dto.pipeline {
        Ok(StepBody::SubPipeline {
            path: pipeline_path.clone(),
            prompt: step_dto.prompt.clone(),
        })
    } else if let Some(ref prompt) = step_dto.prompt {
        Ok(StepBody::Prompt(prompt.clone()))
    } else if step_dto.skill.is_some() {
        Err(cfg_err!(
            "Step '{id_str}' uses 'skill:' which is not yet implemented (planned for v0.2+). \
             Use a 'pipeline:' step to compose pipelines instead."
        ))
    } else if let Some(ref action) = step_dto.action {
        match action.as_str() {
            "pause_for_human" => Ok(StepBody::Action(ActionKind::PauseForHuman)),
            "modify_output" => {
                let headless_behavior = match step_dto.on_headless.as_deref() {
                    None | Some("skip") => HitlHeadlessBehavior::Skip,
                    Some("abort") => HitlHeadlessBehavior::Abort,
                    Some("use_default") => {
                        if step_dto.default_value.is_none() {
                            return Err(cfg_err!(
                                "Step '{id_str}' uses on_headless: use_default but no default_value is provided"
                            ));
                        }
                        HitlHeadlessBehavior::UseDefault
                    }
                    Some(other) => {
                        return Err(cfg_err!(
                            "Step '{id_str}' specifies unknown on_headless value '{other}'; \
                             supported values are 'skip', 'abort', 'use_default'"
                        ))
                    }
                };
                Ok(StepBody::Action(ActionKind::ModifyOutput {
                    headless_behavior,
                    default_value: step_dto.default_value.clone(),
                }))
            }
            other => Err(cfg_err!(
                "Step '{id_str}' specifies unknown action '{other}'"
            )),
        }
    } else if let Some(ref context_dto) = step_dto.context {
        match context_dto.shell {
            Some(ref cmd) => Ok(StepBody::Context(ContextSource::Shell(cmd.clone()))),
            None => Err(cfg_err!(
                "Step '{id_str}' declares context: but no source (shell:, mcp:) is present"
            )),
        }
    } else {
        unreachable!("primary_count == 1 enforced above")
    }
}
