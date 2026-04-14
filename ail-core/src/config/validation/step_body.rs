//! Step body parsing — primary field selection and body construction.

#![allow(clippy::result_large_err)]

use crate::config::domain::{
    ActionKind, ContextSource, HitlHeadlessBehavior, OnMaxItems, StepBody,
};
use crate::config::dto::StepDto;
use crate::error::AilError;

use super::{cfg_err, parse_condition_expression, validate_steps};

/// Parse the step body from a DTO, enforcing exactly one primary field.
///
/// Takes `&mut StepDto` because composite step bodies (e.g. `do_while:`) need to
/// take ownership of nested step lists via `.take()` — cloning is not possible
/// since `StepDto` does not derive `Clone`.
pub(in crate::config) fn parse_step_body(
    step_dto: &mut StepDto,
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
        step_dto.do_while.is_some(),
        step_dto.for_each.is_some(),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if primary_count != 1 {
        return Err(cfg_err!(
            "Step '{id_str}' must have exactly one primary field \
             (prompt, skill, pipeline, action, context, do_while, or for_each); found {primary_count}"
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
    } else if let Some(ref skill_name) = step_dto.skill {
        if skill_name.trim().is_empty() {
            return Err(cfg_err!(
                "Step '{id_str}' declares skill: but the skill name is empty"
            ));
        }
        Ok(StepBody::Skill {
            name: skill_name.clone(),
        })
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
    } else if step_dto.do_while.is_some() {
        parse_do_while_body(step_dto.do_while.take().unwrap(), id_str)
    } else if step_dto.for_each.is_some() {
        parse_for_each_body(step_dto.for_each.take().unwrap(), id_str)
    } else {
        unreachable!("primary_count == 1 enforced above")
    }
}

/// Parse a `do_while:` step body, validating all required fields (SPEC §27).
///
/// Takes ownership of the `DoWhileDto` because the inner `steps` list
/// is passed by value to `validate_steps`.
fn parse_do_while_body(
    dw: crate::config::dto::DoWhileDto,
    id_str: &str,
) -> Result<StepBody, AilError> {
    let max_iterations = dw.max_iterations.ok_or_else(|| {
        cfg_err!(
            "Step '{id_str}' declares do_while: but 'max_iterations' is missing; \
             max_iterations is required to prevent unbounded loops (SPEC §27)"
        )
    })?;

    if max_iterations < 1 {
        return Err(cfg_err!(
            "Step '{id_str}' specifies do_while.max_iterations: 0; \
             max_iterations must be at least 1"
        ));
    }

    let exit_when_raw = dw.exit_when.as_deref().ok_or_else(|| {
        cfg_err!(
            "Step '{id_str}' declares do_while: but 'exit_when' is missing; \
             exit_when is required (SPEC §27)"
        )
    })?;

    // Reuse the condition expression parser from §12.2.
    let exit_when_condition = parse_condition_expression(exit_when_raw, id_str)?;
    // parse_condition_expression returns Condition::Expression for valid expressions.
    // Extract the inner ConditionExpr.
    let exit_when = match exit_when_condition {
        crate::config::domain::Condition::Expression(expr) => expr,
        _ => {
            return Err(cfg_err!(
                "Step '{id_str}' do_while.exit_when must be a condition expression \
                 (e.g. '{{{{ step.<id>.exit_code }}}} == 0'), not 'always' or 'never'"
            ))
        }
    };

    let step_dtos = dw.steps.ok_or_else(|| {
        cfg_err!(
            "Step '{id_str}' declares do_while: but 'steps' is missing; \
             at least one inner step is required (SPEC §27)"
        )
    })?;

    if step_dtos.is_empty() {
        return Err(cfg_err!(
            "Step '{id_str}' declares do_while: with an empty 'steps' array; \
             at least one inner step is required (SPEC §27)"
        ));
    }

    let context_label = format!("do_while step '{id_str}'");
    let inner_steps = validate_steps(step_dtos, &context_label)?;

    Ok(StepBody::DoWhile {
        max_iterations,
        exit_when,
        steps: inner_steps,
    })
}

/// Parse a `for_each:` step body, validating all required fields (SPEC §28).
///
/// Takes ownership of the `ForEachDto` because the inner `steps` list
/// is passed by value to `validate_steps`.
fn parse_for_each_body(
    fe: crate::config::dto::ForEachDto,
    id_str: &str,
) -> Result<StepBody, AilError> {
    let over = fe.over.ok_or_else(|| {
        cfg_err!(
            "Step '{id_str}' declares for_each: but 'over' is missing; \
             over is required and must reference a step's .items (SPEC §28)"
        )
    })?;

    if over.trim().is_empty() {
        return Err(cfg_err!(
            "Step '{id_str}' declares for_each.over: but the value is empty"
        ));
    }

    let as_name = fe.as_name.unwrap_or_else(|| "item".to_string());

    // Validate `as` is a valid identifier.
    if as_name.is_empty()
        || as_name.starts_with(|c: char| c.is_ascii_digit())
        || !as_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(cfg_err!(
            "Step '{id_str}' declares for_each.as: '{as_name}' which is not a valid identifier \
             (must be letters, digits, underscores; cannot start with a digit)"
        ));
    }

    if let Some(max_items) = fe.max_items {
        if max_items < 1 {
            return Err(cfg_err!(
                "Step '{id_str}' specifies for_each.max_items: 0; \
                 max_items must be at least 1"
            ));
        }
    }

    let on_max_items = match fe.on_max_items.as_deref() {
        None | Some("continue") => OnMaxItems::Continue,
        Some("abort_pipeline") => OnMaxItems::AbortPipeline,
        Some(other) => {
            return Err(cfg_err!(
                "Step '{id_str}' specifies unknown for_each.on_max_items value '{other}'; \
                 supported values are 'continue' and 'abort_pipeline'"
            ));
        }
    };

    let step_dtos = fe.steps.ok_or_else(|| {
        cfg_err!(
            "Step '{id_str}' declares for_each: but 'steps' is missing; \
             at least one inner step is required (SPEC §28)"
        )
    })?;

    if step_dtos.is_empty() {
        return Err(cfg_err!(
            "Step '{id_str}' declares for_each: with an empty 'steps' array; \
             at least one inner step is required (SPEC §28)"
        ));
    }

    let context_label = format!("for_each step '{id_str}'");
    let inner_steps = validate_steps(step_dtos, &context_label)?;

    Ok(StepBody::ForEach {
        over,
        as_name,
        max_items: fe.max_items,
        on_max_items,
        steps: inner_steps,
    })
}
