#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::path::PathBuf;

use super::domain::{
    ActionKind, Condition, ContextSource, ExitCodeMatch, Pipeline, ProviderConfig, ResultAction,
    ResultBranch, ResultMatcher, Step, StepBody, StepId, SystemPromptEntry, ToolPolicy,
};
use super::dto::{AppendSystemPromptEntryDto, ExitCodeDto, PipelineFileDto};
use crate::error::{error_types, AilError};

pub fn validate(dto: PipelineFileDto, source: PathBuf) -> Result<Pipeline, AilError> {
    // Resolve top-level defaults (provider/model config, tool policy, and timeout).
    let timeout_seconds = dto.defaults.as_ref().and_then(|d| d.timeout_seconds);
    let (defaults, default_tools) = dto
        .defaults
        .map(|d| {
            let provider_config = ProviderConfig {
                model: d.model,
                base_url: d.provider.as_ref().and_then(|p| p.base_url.clone()),
                auth_token: d.provider.as_ref().and_then(|p| p.auth_token.clone()),
                input_cost_per_1k: d.provider.as_ref().and_then(|p| p.input_cost_per_1k),
                output_cost_per_1k: d.provider.as_ref().and_then(|p| p.output_cost_per_1k),
            };
            let tool_policy = d.tools.map(|t| ToolPolicy {
                allow: t.allow,
                deny: t.deny,
            });
            (provider_config, tool_policy)
        })
        .unwrap_or_else(|| (ProviderConfig::default(), None));

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

    // invocation step, if present, must be first (SPEC §4.1)
    let invocation_pos = step_dtos
        .iter()
        .position(|s| s.id.as_deref() == Some("invocation"));
    if let Some(pos) = invocation_pos {
        if pos != 0 {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "invocation step must be first",
                detail: "The 'invocation' step, if declared, must be the first step in the pipeline (SPEC §4.1)".to_string(),
                context: None,
            });
        }
    }

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
            step_dto.context.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if primary_count != 1 {
            return Err(AilError {
                error_type: error_types::CONFIG_VALIDATION_FAILED,
                title: "Invalid step primary field",
                detail: format!(
                    "Step '{id_str}' must have exactly one primary field (prompt, skill, pipeline, action, or context); found {primary_count}"
                ),
                context: None,
            });
        }

        let body = if let Some(prompt) = step_dto.prompt {
            StepBody::Prompt(prompt)
        } else if let Some(skill) = step_dto.skill {
            StepBody::Skill(PathBuf::from(skill))
        } else if let Some(pipeline) = step_dto.pipeline {
            StepBody::SubPipeline(pipeline)
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
        } else if let Some(context_dto) = step_dto.context {
            match context_dto.shell {
                Some(cmd) => StepBody::Context(ContextSource::Shell(cmd)),
                None => {
                    return Err(AilError {
                        error_type: error_types::CONFIG_VALIDATION_FAILED,
                        title: "context step missing source",
                        detail: format!(
                        "Step '{id_str}' declares context: but no source (shell:, mcp:) is present"
                    ),
                        context: None,
                    })
                }
            }
        } else {
            unreachable!("primary_count == 1 enforced above")
        };

        let tools = step_dto.tools.map(|t| ToolPolicy {
            allow: t.allow,
            deny: t.deny,
        });

        let on_result = step_dto
            .on_result
            .map(|branches| {
                branches
                    .into_iter()
                    .enumerate()
                    .map(|(i, branch)| {
                        let matcher_count = [
                            branch.contains.is_some(),
                            branch.exit_code.is_some(),
                            branch.always.is_some(),
                        ]
                        .iter()
                        .filter(|&&b| b)
                        .count();

                        if matcher_count != 1 {
                            return Err(AilError {
                                error_type: error_types::CONFIG_VALIDATION_FAILED,
                                title: "Invalid on_result branch",
                                detail: format!(
                                    "Step '{id_str}' on_result branch {i} must have exactly one matcher (contains, exit_code, always); found {matcher_count}"
                                ),
                                context: None,
                            });
                        }

                        let action_str = branch.action.ok_or_else(|| AilError {
                            error_type: error_types::CONFIG_VALIDATION_FAILED,
                            title: "on_result branch missing action",
                            detail: format!(
                                "Step '{id_str}' on_result branch {i} must declare an 'action'"
                            ),
                            context: None,
                        })?;

                        let action = if let Some(path) =
                            action_str.strip_prefix("pipeline:").map(str::trim)
                        {
                            if path.is_empty() {
                                return Err(AilError {
                                    error_type: error_types::CONFIG_VALIDATION_FAILED,
                                    title: "pipeline: action missing path",
                                    detail: format!(
                                        "Step '{id_str}' on_result branch {i} action 'pipeline:' requires a path"
                                    ),
                                    context: None,
                                });
                            }
                            ResultAction::Pipeline(path.to_string())
                        } else {
                            match action_str.as_str() {
                                "continue" => ResultAction::Continue,
                                "break" => ResultAction::Break,
                                "abort_pipeline" => ResultAction::AbortPipeline,
                                "pause_for_human" => ResultAction::PauseForHuman,
                                other => {
                                    return Err(AilError {
                                        error_type: error_types::CONFIG_VALIDATION_FAILED,
                                        title: "Unknown on_result action",
                                        detail: format!(
                                            "Step '{id_str}' on_result branch {i} specifies unknown action '{other}'"
                                        ),
                                        context: None,
                                    })
                                }
                            }
                        };

                        let matcher = if let Some(text) = branch.contains {
                            ResultMatcher::Contains(text)
                        } else if let Some(exit_code_dto) = branch.exit_code {
                            let exit_code_match = match exit_code_dto {
                                ExitCodeDto::Integer(n) => ExitCodeMatch::Exact(n),
                                ExitCodeDto::Keyword(k) if k == "any" => ExitCodeMatch::Any,
                                ExitCodeDto::Keyword(k) => {
                                    return Err(AilError {
                                        error_type: error_types::CONFIG_VALIDATION_FAILED,
                                        title: "Invalid exit_code value",
                                        detail: format!(
                                            "Step '{id_str}' on_result branch {i} exit_code must be an integer or 'any', got '{k}'"
                                        ),
                                        context: None,
                                    })
                                }
                            };
                            ResultMatcher::ExitCode(exit_code_match)
                        } else {
                            ResultMatcher::Always
                        };

                        Ok(ResultBranch { matcher, action })
                    })
                    .collect::<Result<Vec<_>, AilError>>()
            })
            .transpose()?;

        let condition = match step_dto.condition.as_deref() {
            None | Some("always") => None,
            Some("never") => Some(Condition::Never),
            Some(other) => {
                return Err(AilError {
                    error_type: error_types::CONFIG_VALIDATION_FAILED,
                    title: "Unknown condition value",
                    detail: format!(
                        "Step '{id_str}' specifies unknown condition '{other}'; supported values are 'always' and 'never'"
                    ),
                    context: None,
                })
            }
        };
        let append_system_prompt = step_dto
            .append_system_prompt
            .map(|entries| {
                entries
                    .into_iter()
                    .enumerate()
                    .map(|(i, entry)| match entry {
                        AppendSystemPromptEntryDto::Text(s) => Ok(SystemPromptEntry::Text(s)),
                        AppendSystemPromptEntryDto::Structured(s) => {
                            let set_count = [s.text.is_some(), s.file.is_some(), s.shell.is_some()]
                                .iter()
                                .filter(|&&b| b)
                                .count();
                            if set_count != 1 {
                                return Err(AilError {
                                    error_type: error_types::CONFIG_VALIDATION_FAILED,
                                    title: "Invalid append_system_prompt entry",
                                    detail: format!(
                                        "Step '{id_str}' append_system_prompt entry {i} must have exactly one key (text, file, or shell); found {set_count}"
                                    ),
                                    context: None,
                                });
                            }
                            if let Some(text) = s.text {
                                Ok(SystemPromptEntry::Text(text))
                            } else if let Some(file) = s.file {
                                Ok(SystemPromptEntry::File(std::path::PathBuf::from(file)))
                            } else if let Some(shell) = s.shell {
                                Ok(SystemPromptEntry::Shell(shell))
                            } else {
                                unreachable!("set_count == 1 enforced above")
                            }
                        }
                    })
                    .collect::<Result<Vec<_>, AilError>>()
            })
            .transpose()?;

        steps.push(Step {
            id: StepId(id_str),
            body,
            message: step_dto.message,
            tools,
            on_result,
            model: step_dto.model,
            runner: step_dto.runner,
            condition,
            append_system_prompt,
            system_prompt: step_dto.system_prompt,
            resume: step_dto.resume.unwrap_or(false),
        });
    }

    Ok(Pipeline {
        steps,
        source: Some(source),
        defaults,
        timeout_seconds,
        default_tools,
    })
}
