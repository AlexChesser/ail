#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::path::PathBuf;

use super::domain::{ActionKind, Pipeline, Step, StepBody, StepId};
use super::dto::PipelineFileDto;
use crate::error::{error_types, AilError};

pub fn validate(dto: PipelineFileDto, source: PathBuf) -> Result<Pipeline, AilError> {
    // version must be present and non-empty
    match &dto.version {
        None => {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Missing version field",
                detail: "The 'version' field is required".to_string(),
                context: None,
            })
        }
        Some(v) if v.trim().is_empty() => {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Empty version field",
                detail: "The 'version' field must not be empty".to_string(),
                context: None,
            })
        }
        _ => {}
    }

    // pipeline array must be present and non-empty
    let step_dtos = match dto.pipeline {
        None => {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Missing pipeline field",
                detail: "The 'pipeline' array is required and must contain at least one step"
                    .to_string(),
                context: None,
            })
        }
        Some(v) if v.is_empty() => {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Empty pipeline",
                detail: "The 'pipeline' array must contain at least one step".to_string(),
                context: None,
            })
        }
        Some(v) => v,
    };

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut steps: Vec<Step> = Vec::with_capacity(step_dtos.len());

    for step_dto in step_dtos {
        let id_str = step_dto.id.ok_or_else(|| AilError {
            error_type: error_types::CONFIG_VALIDATION_FAILED,
            title: "Step missing id",
            detail: "Every step must declare an 'id' field".to_string(),
            context: None,
        })?;

        if !seen_ids.insert(id_str.clone()) {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Duplicate step id",
                detail: format!("Step id '{id_str}' appears more than once"),
                context: None,
            });
        }

        let primary_count = [
            step_dto.prompt.is_some(),
            step_dto.skill.is_some(),
            step_dto.pipeline.is_some(),
            step_dto.action.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if primary_count != 1 {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Invalid step primary field",
                detail: format!(
                    "Step '{id_str}' must have exactly one primary field (prompt, skill, pipeline, or action); found {primary_count}"
                ),
                context: None,
            });
        }

        let body = if let Some(prompt) = step_dto.prompt {
            StepBody::Prompt(prompt)
        } else if let Some(skill) = step_dto.skill {
            StepBody::Skill(PathBuf::from(skill))
        } else if let Some(pipeline) = step_dto.pipeline {
            StepBody::SubPipeline(PathBuf::from(pipeline))
        } else if let Some(action) = step_dto.action {
            match action.as_str() {
                "pause_for_human" => StepBody::Action(ActionKind::PauseForHuman),
                other => {
                    return Err(AilError {
                        error_type: error_types::CONFIG_VALIDATION_FAILED,
                        title: "Unknown action",
                        detail: format!("Step '{id_str}' specifies unknown action '{other}'"),
                        context: None,
                    })
                }
            }
        } else {
            unreachable!("primary_count == 1 enforced above")
        };

        steps.push(Step {
            id: StepId(id_str),
            body,
        });
    }

    Ok(Pipeline {
        steps,
        source: Some(source),
    })
}
