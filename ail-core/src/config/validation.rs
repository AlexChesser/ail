#![allow(clippy::result_large_err)]

use std::collections::HashSet;
use std::path::PathBuf;

use super::domain::{
    ActionKind, Condition, ContextSource, ExitCodeMatch, Pipeline, ProviderConfig, ResultAction,
    ResultBranch, ResultMatcher, Step, StepBody, StepId, SystemPromptEntry, ToolPolicy,
};
use super::dto::{
    AppendSystemPromptEntryDto, ExitCodeDto, OnResultBranchDto, PipelineFileDto, ToolsDto,
};
use crate::error::AilError;

macro_rules! cfg_err {
    ($($arg:tt)*) => {
        AilError::ConfigValidationFailed {
            detail: format!($($arg)*),
            context: None,
        }
    };
}

fn tools_to_policy(t: ToolsDto) -> ToolPolicy {
    ToolPolicy {
        disabled: t.disabled,
        allow: t.allow,
        deny: t.deny,
    }
}

fn parse_result_branches(
    step_id: &str,
    branches: Vec<OnResultBranchDto>,
) -> Result<Vec<ResultBranch>, AilError> {
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
                return Err(cfg_err!(
                    "Step '{step_id}' on_result branch {i} must have exactly one matcher \
                     (contains, exit_code, always); found {matcher_count}"
                ));
            }

            let action_str = branch.action.ok_or_else(|| {
                cfg_err!("Step '{step_id}' on_result branch {i} must declare an 'action'")
            })?;

            let action = if let Some(path) = action_str.strip_prefix("pipeline:").map(str::trim) {
                if path.is_empty() {
                    return Err(cfg_err!(
                        "Step '{step_id}' on_result branch {i} action 'pipeline:' requires a path"
                    ));
                }
                ResultAction::Pipeline {
                    path: path.to_string(),
                    prompt: branch.prompt,
                }
            } else {
                match action_str.as_str() {
                    "continue" => ResultAction::Continue,
                    "break" => ResultAction::Break,
                    "abort_pipeline" => ResultAction::AbortPipeline,
                    "pause_for_human" => ResultAction::PauseForHuman,
                    other => {
                        return Err(cfg_err!(
                        "Step '{step_id}' on_result branch {i} specifies unknown action '{other}'"
                    ))
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
                        return Err(cfg_err!(
                            "Step '{step_id}' on_result branch {i} exit_code must be an integer \
                             or 'any', got '{k}'"
                        ))
                    }
                };
                ResultMatcher::ExitCode(exit_code_match)
            } else {
                ResultMatcher::Always
            };

            Ok(ResultBranch { matcher, action })
        })
        .collect()
}

fn parse_append_system_prompt(
    step_id: &str,
    entries: Vec<AppendSystemPromptEntryDto>,
) -> Result<Vec<SystemPromptEntry>, AilError> {
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
                    return Err(cfg_err!(
                        "Step '{step_id}' append_system_prompt entry {i} must have exactly one \
                         key (text, file, or shell); found {set_count}"
                    ));
                }
                if let Some(text) = s.text {
                    Ok(SystemPromptEntry::Text(text))
                } else if let Some(file) = s.file {
                    Ok(SystemPromptEntry::File(std::path::PathBuf::from(file)))
                } else {
                    Ok(SystemPromptEntry::Shell(s.shell.expect("set_count == 1")))
                }
            }
        })
        .collect()
}

pub fn validate(dto: PipelineFileDto, source: PathBuf) -> Result<Pipeline, AilError> {
    // Resolve top-level defaults (provider/model config, tool policy, and timeout).
    let timeout_seconds = dto.defaults.as_ref().and_then(|d| d.timeout_seconds);
    let (defaults, default_tools) = dto
        .defaults
        .map(|d| {
            let provider_config = ProviderConfig {
                model: d
                    .provider
                    .as_ref()
                    .and_then(|p| p.model.clone())
                    .or(d.model),
                base_url: d.provider.as_ref().and_then(|p| p.base_url.clone()),
                auth_token: d.provider.as_ref().and_then(|p| p.auth_token.clone()),
                input_cost_per_1k: d.provider.as_ref().and_then(|p| p.input_cost_per_1k),
                output_cost_per_1k: d.provider.as_ref().and_then(|p| p.output_cost_per_1k),
            };
            (provider_config, d.tools.map(tools_to_policy))
        })
        .unwrap_or_else(|| (ProviderConfig::default(), None));

    // version must be present and non-empty
    match &dto.version {
        None => return Err(cfg_err!("The 'version' field is required")),
        Some(v) if v.trim().is_empty() => {
            return Err(cfg_err!("The 'version' field must not be empty"))
        }
        _ => {}
    }

    // pipeline array must be present and non-empty
    let step_dtos = match dto.pipeline {
        None => {
            return Err(cfg_err!(
                "The 'pipeline' array is required and must contain at least one step"
            ))
        }
        Some(v) if v.is_empty() => {
            return Err(cfg_err!(
                "The 'pipeline' array must contain at least one step"
            ))
        }
        Some(v) => v,
    };

    // invocation step, if present, must be first (SPEC §4.1)
    if let Some(pos) = step_dtos
        .iter()
        .position(|s| s.id.as_deref() == Some("invocation"))
    {
        if pos != 0 {
            return Err(cfg_err!(
                "The 'invocation' step, if declared, must be the first step in the pipeline (SPEC §4.1)"
            ));
        }
    }

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut steps: Vec<Step> = Vec::with_capacity(step_dtos.len());

    for step_dto in step_dtos {
        let id_str = step_dto
            .id
            .ok_or_else(|| cfg_err!("Every step must declare an 'id' field"))?;

        if !seen_ids.insert(id_str.clone()) {
            return Err(cfg_err!("Step id '{id_str}' appears more than once"));
        }

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
        let body = if let Some(pipeline_path) = step_dto.pipeline {
            StepBody::SubPipeline {
                path: pipeline_path,
                prompt: step_dto.prompt,
            }
        } else if let Some(prompt) = step_dto.prompt {
            StepBody::Prompt(prompt)
        } else if let Some(skill) = step_dto.skill {
            StepBody::Skill(PathBuf::from(skill))
        } else if let Some(action) = step_dto.action {
            match action.as_str() {
                "pause_for_human" => StepBody::Action(ActionKind::PauseForHuman),
                other => {
                    return Err(cfg_err!(
                        "Step '{id_str}' specifies unknown action '{other}'"
                    ))
                }
            }
        } else if let Some(context_dto) = step_dto.context {
            match context_dto.shell {
                Some(cmd) => StepBody::Context(ContextSource::Shell(cmd)),
                None => {
                    return Err(cfg_err!(
                        "Step '{id_str}' declares context: but no source (shell:, mcp:) is present"
                    ))
                }
            }
        } else {
            unreachable!("primary_count == 1 enforced above")
        };

        let on_result = step_dto
            .on_result
            .map(|branches| parse_result_branches(&id_str, branches))
            .transpose()?;

        let condition = match step_dto.condition.as_deref() {
            None | Some("always") => None,
            Some("never") => Some(Condition::Never),
            Some(other) => {
                return Err(cfg_err!(
                    "Step '{id_str}' specifies unknown condition '{other}'; \
                     supported values are 'always' and 'never'"
                ))
            }
        };

        let append_system_prompt = step_dto
            .append_system_prompt
            .map(|entries| parse_append_system_prompt(&id_str, entries))
            .transpose()?;

        steps.push(Step {
            id: StepId(id_str),
            body,
            message: step_dto.message,
            tools: step_dto.tools.map(tools_to_policy),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::dto::{
        AppendSystemPromptEntryDto, AppendSystemPromptStructuredDto, ContextDto, DefaultsDto,
        ExitCodeDto, OnResultBranchDto, PipelineFileDto, ProviderDto, StepDto, ToolsDto,
    };
    use crate::error::error_types;

    fn source() -> PathBuf {
        PathBuf::from("/tmp/test.ail.yaml")
    }

    fn minimal_step(id: &str, prompt: &str) -> StepDto {
        StepDto {
            id: Some(id.to_string()),
            prompt: Some(prompt.to_string()),
            skill: None,
            pipeline: None,
            action: None,
            message: None,
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }
    }

    fn minimal_dto(steps: Vec<StepDto>) -> PipelineFileDto {
        PipelineFileDto {
            version: Some("1".to_string()),
            defaults: None,
            pipeline: Some(steps),
        }
    }

    // ── valid minimal pipeline ───────────────────────────────────────────────

    #[test]
    fn valid_minimal_pipeline_returns_ok() {
        let dto = minimal_dto(vec![minimal_step("review", "Please review the code")]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.steps.len(), 1);
        assert_eq!(pipeline.steps[0].id.as_str(), "review");
    }

    // ── version field ────────────────────────────────────────────────────────

    #[test]
    fn missing_version_returns_error() {
        let dto = PipelineFileDto {
            version: None,
            defaults: None,
            pipeline: Some(vec![minimal_step("s1", "hello")]),
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("version"));
    }

    #[test]
    fn empty_version_returns_error() {
        let dto = PipelineFileDto {
            version: Some("   ".to_string()),
            defaults: None,
            pipeline: Some(vec![minimal_step("s1", "hello")]),
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── pipeline array ───────────────────────────────────────────────────────

    #[test]
    fn missing_pipeline_field_returns_error() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: None,
            pipeline: None,
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("pipeline"));
    }

    #[test]
    fn empty_pipeline_array_returns_error() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: None,
            pipeline: Some(vec![]),
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── step id validation ───────────────────────────────────────────────────

    #[test]
    fn step_missing_id_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            id: None,
            prompt: Some("hello".to_string()),
            ..minimal_step("unused", "unused")
        }]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("id"));
    }

    #[test]
    fn duplicate_step_id_returns_error() {
        let dto = minimal_dto(vec![
            minimal_step("dupe", "first"),
            minimal_step("dupe", "second"),
        ]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("dupe"));
    }

    // ── invocation step ordering ─────────────────────────────────────────────

    #[test]
    fn invocation_step_not_first_returns_error() {
        let dto = minimal_dto(vec![
            minimal_step("first", "hello"),
            minimal_step("invocation", "world"),
        ]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("invocation"));
    }

    #[test]
    fn invocation_step_first_is_valid() {
        let dto = minimal_dto(vec![
            minimal_step("invocation", "kick off"),
            minimal_step("second", "continue"),
        ]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.steps[0].id.as_str(), "invocation");
    }

    // ── step primary field count ─────────────────────────────────────────────

    #[test]
    fn step_with_no_primary_field_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("empty".to_string()),
            prompt: None,
            skill: None,
            pipeline: None,
            action: None,
            message: None,
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("empty"));
    }

    #[test]
    fn step_with_two_primary_fields_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("conflict".to_string()),
            prompt: Some("hello".to_string()),
            skill: Some("my-skill".to_string()),
            pipeline: None,
            action: None,
            message: None,
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── step body types ──────────────────────────────────────────────────────

    #[test]
    fn skill_step_round_trips() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("sk".to_string()),
            skill: Some("my-skill.yaml".to_string()),
            prompt: None,
            pipeline: None,
            action: None,
            message: None,
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(matches!(pipeline.steps[0].body, StepBody::Skill(_)));
    }

    #[test]
    fn sub_pipeline_step_round_trips() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("sub".to_string()),
            pipeline: Some("child.ail.yaml".to_string()),
            prompt: None,
            skill: None,
            action: None,
            message: None,
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(matches!(
            pipeline.steps[0].body,
            StepBody::SubPipeline { .. }
        ));
    }

    #[test]
    fn action_pause_for_human_round_trips() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("gate".to_string()),
            action: Some("pause_for_human".to_string()),
            prompt: None,
            skill: None,
            pipeline: None,
            message: Some("Waiting for approval".to_string()),
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(matches!(
            pipeline.steps[0].body,
            StepBody::Action(ActionKind::PauseForHuman)
        ));
    }

    #[test]
    fn unknown_action_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("bad-action".to_string()),
            action: Some("fly_to_moon".to_string()),
            prompt: None,
            skill: None,
            pipeline: None,
            message: None,
            context: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("fly_to_moon"));
    }

    #[test]
    fn context_shell_step_round_trips() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("ctx".to_string()),
            context: Some(ContextDto {
                shell: Some("git status".to_string()),
            }),
            prompt: None,
            skill: None,
            pipeline: None,
            action: None,
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(matches!(
            pipeline.steps[0].body,
            StepBody::Context(ContextSource::Shell(_))
        ));
    }

    #[test]
    fn context_step_without_source_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("ctx".to_string()),
            context: Some(ContextDto { shell: None }),
            prompt: None,
            skill: None,
            pipeline: None,
            action: None,
            message: None,
            tools: None,
            on_result: None,
            model: None,
            runner: None,
            condition: None,
            append_system_prompt: None,
            system_prompt: None,
            resume: None,
        }]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("context"));
    }

    // ── condition field ──────────────────────────────────────────────────────

    #[test]
    fn condition_always_is_accepted_and_returns_none() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("always".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(pipeline.steps[0].condition.is_none());
    }

    #[test]
    fn condition_never_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("never".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.steps[0].condition, Some(Condition::Never));
    }

    #[test]
    fn condition_unknown_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("maybe".to_string());
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("maybe"));
    }

    // ── on_result branches ───────────────────────────────────────────────────

    #[test]
    fn on_result_contains_continue_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("SUCCESS".to_string()),
            exit_code: None,
            always: None,
            action: Some("continue".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
        assert!(matches!(branch.matcher, ResultMatcher::Contains(_)));
        assert!(matches!(branch.action, ResultAction::Continue));
    }

    #[test]
    fn on_result_exit_code_exact_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: None,
            exit_code: Some(ExitCodeDto::Integer(1)),
            always: None,
            action: Some("break".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
        assert!(matches!(
            branch.matcher,
            ResultMatcher::ExitCode(ExitCodeMatch::Exact(1))
        ));
        assert!(matches!(branch.action, ResultAction::Break));
    }

    #[test]
    fn on_result_exit_code_any_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: None,
            exit_code: Some(ExitCodeDto::Keyword("any".to_string())),
            always: None,
            action: Some("abort_pipeline".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
        assert!(matches!(
            branch.matcher,
            ResultMatcher::ExitCode(ExitCodeMatch::Any)
        ));
        assert!(matches!(branch.action, ResultAction::AbortPipeline));
    }

    #[test]
    fn on_result_exit_code_invalid_keyword_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: None,
            exit_code: Some(ExitCodeDto::Keyword("none".to_string())),
            always: None,
            action: Some("continue".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("none"));
    }

    #[test]
    fn on_result_always_matcher_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: None,
            exit_code: None,
            always: Some(true),
            action: Some("pause_for_human".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
        assert!(matches!(branch.matcher, ResultMatcher::Always));
        assert!(matches!(branch.action, ResultAction::PauseForHuman));
    }

    #[test]
    fn on_result_pipeline_action_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("ok".to_string()),
            exit_code: None,
            always: None,
            action: Some("pipeline: child.ail.yaml".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let branch = &pipeline.steps[0].on_result.as_ref().unwrap()[0];
        assert!(matches!(branch.action, ResultAction::Pipeline { .. }));
    }

    #[test]
    fn on_result_pipeline_action_empty_path_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("ok".to_string()),
            exit_code: None,
            always: None,
            action: Some("pipeline:".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("path"));
    }

    #[test]
    fn on_result_unknown_action_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("ok".to_string()),
            exit_code: None,
            always: None,
            action: Some("teleport".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("teleport"));
    }

    #[test]
    fn on_result_branch_missing_action_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("ok".to_string()),
            exit_code: None,
            always: None,
            action: None,
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("action"));
    }

    #[test]
    fn on_result_branch_zero_matchers_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: None,
            exit_code: None,
            always: None,
            action: Some("continue".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    #[test]
    fn on_result_branch_two_matchers_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("ok".to_string()),
            exit_code: Some(ExitCodeDto::Integer(0)),
            always: None,
            action: Some("continue".to_string()),
            prompt: None,
        }]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── defaults / ProviderConfig ────────────────────────────────────────────

    #[test]
    fn defaults_model_field_propagates() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                model: Some("claude-3-5-haiku".to_string()),
                provider: None,
                timeout_seconds: None,
                tools: None,
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.defaults.model.as_deref(), Some("claude-3-5-haiku"));
    }

    #[test]
    fn defaults_provider_model_wins_over_top_level_model() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                model: Some("top-level".to_string()),
                provider: Some(ProviderDto {
                    model: Some("provider-wins".to_string()),
                    base_url: None,
                    auth_token: None,
                    input_cost_per_1k: None,
                    output_cost_per_1k: None,
                }),
                timeout_seconds: None,
                tools: None,
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        // provider.model takes precedence over defaults.model
        assert_eq!(pipeline.defaults.model.as_deref(), Some("provider-wins"));
    }

    #[test]
    fn defaults_no_provider_falls_through_to_top_level_model() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                model: Some("fallback-model".to_string()),
                provider: None,
                timeout_seconds: None,
                tools: None,
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.defaults.model.as_deref(), Some("fallback-model"));
    }

    #[test]
    fn timeout_seconds_propagates() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                model: None,
                provider: None,
                timeout_seconds: Some(120),
                tools: None,
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.timeout_seconds, Some(120));
    }

    #[test]
    fn default_tools_propagate() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                model: None,
                provider: None,
                timeout_seconds: None,
                tools: Some(ToolsDto {
                    disabled: false,
                    allow: vec!["Bash".to_string()],
                    deny: vec![],
                }),
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        let default_tools = pipeline.default_tools.expect("should have default_tools");
        assert_eq!(default_tools.allow, vec!["Bash".to_string()]);
    }

    // ── append_system_prompt ─────────────────────────────────────────────────

    #[test]
    fn append_system_prompt_text_shorthand_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.append_system_prompt = Some(vec![AppendSystemPromptEntryDto::Text(
            "Be concise".to_string(),
        )]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let entries = pipeline.steps[0]
            .append_system_prompt
            .as_ref()
            .expect("should have entries");
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0], SystemPromptEntry::Text(_)));
    }

    #[test]
    fn append_system_prompt_structured_text_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.append_system_prompt = Some(vec![AppendSystemPromptEntryDto::Structured(
            AppendSystemPromptStructuredDto {
                text: Some("Be helpful".to_string()),
                file: None,
                shell: None,
            },
        )]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let entries = pipeline.steps[0]
            .append_system_prompt
            .as_ref()
            .expect("should have entries");
        assert!(matches!(entries[0], SystemPromptEntry::Text(_)));
    }

    #[test]
    fn append_system_prompt_structured_file_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.append_system_prompt = Some(vec![AppendSystemPromptEntryDto::Structured(
            AppendSystemPromptStructuredDto {
                text: None,
                file: Some("rules.txt".to_string()),
                shell: None,
            },
        )]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let entries = pipeline.steps[0]
            .append_system_prompt
            .as_ref()
            .expect("should have entries");
        assert!(matches!(entries[0], SystemPromptEntry::File(_)));
    }

    #[test]
    fn append_system_prompt_structured_shell_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.append_system_prompt = Some(vec![AppendSystemPromptEntryDto::Structured(
            AppendSystemPromptStructuredDto {
                text: None,
                file: None,
                shell: Some("cat rules.txt".to_string()),
            },
        )]);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let entries = pipeline.steps[0]
            .append_system_prompt
            .as_ref()
            .expect("should have entries");
        assert!(matches!(entries[0], SystemPromptEntry::Shell(_)));
    }

    #[test]
    fn append_system_prompt_structured_zero_keys_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.append_system_prompt = Some(vec![AppendSystemPromptEntryDto::Structured(
            AppendSystemPromptStructuredDto {
                text: None,
                file: None,
                shell: None,
            },
        )]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    #[test]
    fn append_system_prompt_structured_two_keys_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.append_system_prompt = Some(vec![AppendSystemPromptEntryDto::Structured(
            AppendSystemPromptStructuredDto {
                text: Some("inline".to_string()),
                file: Some("also-file.txt".to_string()),
                shell: None,
            },
        )]);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── resume field ─────────────────────────────────────────────────────────

    #[test]
    fn resume_defaults_to_false() {
        let dto = minimal_dto(vec![minimal_step("s", "hello")]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(!pipeline.steps[0].resume);
    }

    #[test]
    fn resume_true_propagates() {
        let mut step = minimal_step("s", "hello");
        step.resume = Some(true);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(pipeline.steps[0].resume);
    }

    // ── source path ──────────────────────────────────────────────────────────

    #[test]
    fn source_path_is_set_on_pipeline() {
        let dto = minimal_dto(vec![minimal_step("s", "hello")]);
        let src = PathBuf::from("/custom/path.ail.yaml");
        let pipeline = validate(dto, src.clone()).expect("should succeed");
        assert_eq!(pipeline.source, Some(src));
    }
}
