//! DTO → domain validation for pipeline files.

#![allow(clippy::result_large_err)]

mod on_result;
mod step_body;
mod system_prompt;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::domain::{
    Condition, ConditionExpr, ConditionOp, OnError, Pipeline, ProviderConfig, Step, StepBody,
    StepId, ToolPolicy,
};
use super::dto::{ChainStepDto, PipelineFileDto, StepDto, ToolsDto};
use crate::error::AilError;

macro_rules! cfg_err {
    ($($arg:tt)*) => {
        AilError::ConfigValidationFailed {
            detail: format!($($arg)*),
            context: None,
        }
    };
}

// Make the macro available to sub-modules.
pub(in crate::config) use cfg_err;

fn tools_to_policy(t: ToolsDto) -> ToolPolicy {
    ToolPolicy {
        disabled: t.disabled,
        allow: t.allow,
        deny: t.deny,
    }
}

/// Parse a condition expression string into a `Condition::Expression`.
///
/// Supported operators: `==`, `!=`, `contains`, `starts_with`, `ends_with`.
/// The LHS is typically a template variable (e.g. `{{ step.test.exit_code }}`),
/// and the RHS is a literal or quoted string.
///
/// Examples:
///   `"{{ step.test.exit_code }} == 0"`
///   `"{{ step.review.response }} contains 'LGTM'"`
///   `"{{ step.build.exit_code }} != 0"`
pub(in crate::config) fn parse_condition_expression(
    raw: &str,
    step_id: &str,
) -> Result<Condition, AilError> {
    // Operator tokens in order of specificity (multi-word first to avoid partial matches).
    let operators: &[(&str, ConditionOp)] = &[
        ("starts_with", ConditionOp::StartsWith),
        ("ends_with", ConditionOp::EndsWith),
        ("contains", ConditionOp::Contains),
        ("!=", ConditionOp::Ne),
        ("==", ConditionOp::Eq),
    ];

    for &(token, ref op) in operators {
        if let Some(pos) = find_operator_position(raw, token) {
            let lhs = raw[..pos].trim().to_string();
            let rhs_raw = raw[pos + token.len()..].trim();
            let rhs = strip_quotes(rhs_raw).to_string();

            if lhs.is_empty() {
                return Err(cfg_err!(
                    "Step '{step_id}' condition expression has an empty left-hand side"
                ));
            }
            if rhs.is_empty() {
                return Err(cfg_err!(
                    "Step '{step_id}' condition expression has an empty right-hand side"
                ));
            }

            return Ok(Condition::Expression(ConditionExpr {
                lhs,
                op: op.clone(),
                rhs,
            }));
        }
    }

    Err(cfg_err!(
        "Step '{step_id}' specifies condition '{raw}' which is not a recognised \
         named condition ('always', 'never') and does not contain a supported operator \
         (==, !=, contains, starts_with, ends_with)"
    ))
}

/// Find the position of an operator token in a condition string.
///
/// For word-based operators (`contains`, `starts_with`, `ends_with`), require word
/// boundaries so that template variables like `{{ step.contains_test.response }}`
/// are not treated as operators. `==` and `!=` never appear inside template variables,
/// so they use simple substring search.
fn find_operator_position(raw: &str, token: &str) -> Option<usize> {
    if token == "==" || token == "!=" {
        // For symbolic operators, find the first occurrence outside `{{ }}` blocks.
        return find_outside_templates(raw, token);
    }

    // Word-based operator — scan for occurrences with word boundaries.
    let mut search_from = 0;
    while let Some(rel) = raw[search_from..].find(token) {
        let abs = search_from + rel;

        // Check the char is not inside `{{ ... }}`.
        if is_inside_template(raw, abs) {
            search_from = abs + token.len();
            continue;
        }

        // Check left boundary: must be start-of-string or whitespace.
        let left_ok = abs == 0 || raw.as_bytes()[abs - 1].is_ascii_whitespace();
        // Check right boundary: must be end-of-string or whitespace.
        let right = abs + token.len();
        let right_ok = right >= raw.len() || raw.as_bytes()[right].is_ascii_whitespace();

        if left_ok && right_ok {
            return Some(abs);
        }

        search_from = abs + token.len();
    }
    None
}

/// Find the first occurrence of `needle` that is not inside a `{{ ... }}` block.
fn find_outside_templates(raw: &str, needle: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = raw[search_from..].find(needle) {
        let abs = search_from + rel;
        if !is_inside_template(raw, abs) {
            return Some(abs);
        }
        search_from = abs + needle.len();
    }
    None
}

/// Returns `true` if position `pos` falls inside a `{{ ... }}` block.
fn is_inside_template(raw: &str, pos: usize) -> bool {
    // Find the last `{{` before `pos` and check whether there is a matching `}}` after it
    // but before `pos`.
    let before = &raw[..pos];
    if let Some(open) = before.rfind("{{") {
        // Check if there's a `}}` between that `{{` and `pos`.
        let between = &raw[open + 2..pos];
        !between.contains("}}")
    } else {
        false
    }
}

/// Strip surrounding single or double quotes from a string, if present.
fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')))
    {
        return &s[1..s.len() - 1];
    }
    s
}

/// Validate a list of step DTOs into domain `Step`s.
/// `context_label` is used in error messages to indicate where these steps come from
/// (e.g. "pipeline" or "named pipeline 'security_gates'").
pub(in crate::config) fn validate_steps(
    step_dtos: Vec<StepDto>,
    context_label: &str,
) -> Result<Vec<Step>, AilError> {
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut steps: Vec<Step> = Vec::with_capacity(step_dtos.len());

    for mut step_dto in step_dtos {
        // Reject leftover hook operation fields — these are consumed during FROM
        // inheritance resolution and should never reach validation. If they do,
        // the pipeline either lacks FROM or the merge logic has a bug.
        if step_dto.run_before.is_some()
            || step_dto.run_after.is_some()
            || step_dto.override_step.is_some()
            || step_dto.disable.is_some()
        {
            let hook_field = if step_dto.disable.is_some() {
                format!("disable: {}", step_dto.disable.as_deref().unwrap_or("?"))
            } else if step_dto.override_step.is_some() {
                format!(
                    "override: {}",
                    step_dto.override_step.as_deref().unwrap_or("?")
                )
            } else if step_dto.run_before.is_some() {
                format!(
                    "run_before: {}",
                    step_dto.run_before.as_deref().unwrap_or("?")
                )
            } else {
                format!(
                    "run_after: {}",
                    step_dto.run_after.as_deref().unwrap_or("?")
                )
            };
            return Err(cfg_err!(
                "Step declares hook operation '{hook_field}' but the pipeline has no \
                 FROM field — hook operations are only valid in pipelines that inherit \
                 from a base via FROM (SPEC §7.2)"
            ));
        }

        let id_str = step_dto
            .id
            .clone()
            .ok_or_else(|| cfg_err!("Every step in {context_label} must declare an 'id' field"))?;

        if !seen_ids.insert(id_str.clone()) {
            return Err(cfg_err!(
                "Step id '{id_str}' appears more than once in {context_label}"
            ));
        }

        let body = step_body::parse_step_body(&mut step_dto, &id_str)?;

        let on_result = step_dto
            .on_result
            .map(|branches| on_result::parse_result_branches(&id_str, branches))
            .transpose()?;

        let condition = match step_dto.condition.as_deref() {
            None | Some("always") => None,
            Some("never") => Some(Condition::Never),
            Some(other) => Some(parse_condition_expression(other, &id_str)?),
        };

        let append_system_prompt = step_dto
            .append_system_prompt
            .map(|entries| system_prompt::parse_append_system_prompt(&id_str, entries))
            .transpose()?;

        let on_error = match step_dto.on_error.as_deref() {
            None => None,
            Some("continue") => {
                if step_dto.max_retries.is_some() {
                    return Err(cfg_err!(
                        "Step '{id_str}' specifies 'max_retries' but 'on_error' is 'continue'; \
                         'max_retries' is only valid with 'on_error: retry'"
                    ));
                }
                Some(OnError::Continue)
            }
            Some("retry") => {
                let max_retries = step_dto.max_retries.ok_or_else(|| {
                    cfg_err!(
                        "Step '{id_str}' specifies 'on_error: retry' but 'max_retries' is missing; \
                         'max_retries' is required when 'on_error' is 'retry'"
                    )
                })?;
                if max_retries == 0 {
                    return Err(cfg_err!(
                        "Step '{id_str}' specifies 'max_retries: 0'; \
                         'max_retries' must be at least 1"
                    ));
                }
                Some(OnError::Retry { max_retries })
            }
            Some("abort_pipeline") => {
                if step_dto.max_retries.is_some() {
                    return Err(cfg_err!(
                        "Step '{id_str}' specifies 'max_retries' but 'on_error' is 'abort_pipeline'; \
                         'max_retries' is only valid with 'on_error: retry'"
                    ));
                }
                Some(OnError::AbortPipeline)
            }
            Some(other) => {
                return Err(cfg_err!(
                    "Step '{id_str}' specifies unknown on_error value '{other}'; \
                     supported values are 'continue', 'retry', and 'abort_pipeline'"
                ))
            }
        };

        // max_retries without on_error is an error
        if step_dto.on_error.is_none() && step_dto.max_retries.is_some() {
            return Err(cfg_err!(
                "Step '{id_str}' specifies 'max_retries' without 'on_error'; \
                 'max_retries' is only valid with 'on_error: retry'"
            ));
        }

        let before = parse_chain_steps(step_dto.before, &id_str, "before")?;
        let then = parse_chain_steps(step_dto.then, &id_str, "then")?;

        // Validate output_schema is a valid JSON Schema if present (SPEC §26).
        let output_schema = if let Some(ref schema) = step_dto.output_schema {
            // Attempt to compile the schema — if it's not a valid JSON Schema,
            // this will fail and we produce a clear parse-time error.
            if let Err(e) = jsonschema::validator_for(schema) {
                return Err(cfg_err!(
                    "Step '{id_str}' declares output_schema but it is not a valid \
                     JSON Schema: {e}"
                ));
            }
            Some(schema.clone())
        } else {
            None
        };

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
            on_error,
            before,
            then,
            output_schema,
        });
    }

    Ok(steps)
}

/// Reclassify `StepBody::SubPipeline` references whose path matches a named pipeline
/// key into `StepBody::NamedPipeline`. This is done as a second pass after both the
/// main pipeline steps and named pipeline definitions have been validated.
fn reclassify_named_pipeline_refs(
    steps: &mut [Step],
    named_pipelines: &HashMap<String, Vec<Step>>,
) {
    for step in steps.iter_mut() {
        if let StepBody::SubPipeline {
            ref path,
            ref prompt,
        } = step.body
        {
            if named_pipelines.contains_key(path) {
                step.body = StepBody::NamedPipeline {
                    name: path.clone(),
                    prompt: prompt.clone(),
                };
            }
        }
    }
}

/// Detect circular references among named pipelines via DFS.
/// Returns an error if any cycle is found.
fn detect_named_pipeline_cycles(
    named_pipelines: &HashMap<String, Vec<Step>>,
) -> Result<(), AilError> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();

    for name in named_pipelines.keys() {
        if !visited.contains(name) {
            dfs_cycle_check(name, named_pipelines, &mut visited, &mut in_stack)?;
        }
    }
    Ok(())
}

fn dfs_cycle_check(
    name: &str,
    named_pipelines: &HashMap<String, Vec<Step>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
) -> Result<(), AilError> {
    visited.insert(name.to_string());
    in_stack.insert(name.to_string());

    if let Some(steps) = named_pipelines.get(name) {
        for step in steps {
            if let StepBody::NamedPipeline {
                name: ref dep_name, ..
            } = step.body
            {
                if in_stack.contains(dep_name) {
                    return Err(AilError::PipelineCircularReference {
                        detail: format!(
                            "Circular reference detected among named pipelines: \
                             '{name}' references '{dep_name}' which forms a cycle"
                        ),
                        context: None,
                    });
                }
                if !visited.contains(dep_name) {
                    dfs_cycle_check(dep_name, named_pipelines, visited, in_stack)?;
                }
            }
        }
    }

    in_stack.remove(name);
    Ok(())
}

/// Parse a chain step entry (from `before:` or `then:`) into a domain Step.
///
/// `parent_id` is the parent step's id, `chain_kind` is `"before"` or `"then"`,
/// and `index` is the 0-based index within the chain.
fn parse_chain_step(
    entry: ChainStepDto,
    parent_id: &str,
    chain_kind: &str,
    index: usize,
) -> Result<Step, AilError> {
    let auto_id = format!("{parent_id}::{chain_kind}::{index}");

    match entry {
        ChainStepDto::Short(s) => {
            // Short-form: bare string — treat as a prompt (file path or inline text).
            // Per spec, bare strings can be skill references or prompt file paths.
            // Since skill: is not yet implemented, treat all short-form as prompt.
            Ok(Step {
                id: StepId(auto_id),
                body: StepBody::Prompt(s),
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
                output_schema: None,
            })
        }
        ChainStepDto::Full(mut step_dto) => {
            let body = step_body::parse_step_body(&mut step_dto, &auto_id)?;

            let on_result_branches = step_dto
                .on_result
                .map(|branches| on_result::parse_result_branches(&auto_id, branches))
                .transpose()?;

            let append_system_prompt_entries = step_dto
                .append_system_prompt
                .map(|entries| system_prompt::parse_append_system_prompt(&auto_id, entries))
                .transpose()?;

            // Recursively parse nested before/then chains.
            let before = parse_chain_steps(step_dto.before, &auto_id, "before")?;
            let then = parse_chain_steps(step_dto.then, &auto_id, "then")?;

            Ok(Step {
                id: StepId(auto_id),
                body,
                message: step_dto.message,
                tools: step_dto.tools.map(tools_to_policy),
                on_result: on_result_branches,
                model: step_dto.model,
                runner: step_dto.runner,
                condition: None, // Chain steps inherit condition from parent.
                append_system_prompt: append_system_prompt_entries,
                system_prompt: step_dto.system_prompt,
                resume: step_dto.resume.unwrap_or(false),
                on_error: None,
                before,
                then,
                output_schema: step_dto.output_schema,
            })
        }
    }
}

/// Parse a list of chain step entries (from `before:` or `then:`) into domain Steps.
fn parse_chain_steps(
    entries: Option<Vec<ChainStepDto>>,
    parent_id: &str,
    chain_kind: &str,
) -> Result<Vec<Step>, AilError> {
    match entries {
        None => Ok(vec![]),
        Some(entries) => entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| parse_chain_step(entry, parent_id, chain_kind, i))
            .collect(),
    }
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
                connect_timeout_seconds: d
                    .provider
                    .as_ref()
                    .and_then(|p| p.connect_timeout_seconds),
                read_timeout_seconds: d.provider.as_ref().and_then(|p| p.read_timeout_seconds),
                max_history_messages: d.provider.as_ref().and_then(|p| p.max_history_messages),
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

    let mut steps = validate_steps(step_dtos, "pipeline")?;

    // ── Validate named pipelines (SPEC §10) ─────────────────────────────────
    let mut named_pipelines = if let Some(named_dtos) = dto.pipelines {
        let mut named: HashMap<String, Vec<Step>> = HashMap::with_capacity(named_dtos.len());
        for (name, np_step_dtos) in named_dtos {
            if name.is_empty() {
                return Err(cfg_err!("Named pipeline names must not be empty"));
            }
            if np_step_dtos.is_empty() {
                return Err(cfg_err!(
                    "Named pipeline '{name}' must contain at least one step"
                ));
            }
            let label = format!("named pipeline '{name}'");
            let np_steps = validate_steps(np_step_dtos, &label)?;
            named.insert(name, np_steps);
        }
        named
    } else {
        HashMap::new()
    };

    // ── Second pass: reclassify SubPipeline → NamedPipeline when the path
    //    matches a named pipeline key (SPEC §10). ────────────────────────────
    if !named_pipelines.is_empty() {
        reclassify_named_pipeline_refs(&mut steps, &named_pipelines);
        // Also reclassify refs within named pipelines themselves (cross-references).
        let names: Vec<String> = named_pipelines.keys().cloned().collect();
        for name in &names {
            // Clone the keys to avoid borrow conflict; reclassify needs &HashMap but we
            // also need &mut for the inner steps. Safe because we only mutate the steps,
            // not the map structure.
            let np_keys: HashSet<String> = named_pipelines.keys().cloned().collect();
            if let Some(np_steps) = named_pipelines.get_mut(name) {
                for step in np_steps.iter_mut() {
                    if let StepBody::SubPipeline {
                        ref path,
                        ref prompt,
                    } = step.body
                    {
                        if np_keys.contains(path) {
                            step.body = StepBody::NamedPipeline {
                                name: path.clone(),
                                prompt: prompt.clone(),
                            };
                        }
                    }
                }
            }
        }
        // Detect circular references in named pipelines (SPEC §10).
        detect_named_pipeline_cycles(&named_pipelines)?;
    }

    Ok(Pipeline {
        steps,
        source: Some(source),
        defaults,
        timeout_seconds,
        default_tools,
        named_pipelines,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::domain::{
        ActionKind, Condition, ContextSource, ExitCodeMatch, ResultAction, ResultMatcher, StepBody,
        SystemPromptEntry,
    };
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
            ..Default::default()
        }
    }

    fn minimal_dto(steps: Vec<StepDto>) -> PipelineFileDto {
        PipelineFileDto {
            version: Some("1".to_string()),
            pipeline: Some(steps),
            ..Default::default()
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
            pipeline: Some(vec![minimal_step("s1", "hello")]),
            ..Default::default()
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("version"));
    }

    #[test]
    fn empty_version_returns_error() {
        let dto = PipelineFileDto {
            version: Some("   ".to_string()),
            pipeline: Some(vec![minimal_step("s1", "hello")]),
            ..Default::default()
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── pipeline array ───────────────────────────────────────────────────────

    #[test]
    fn missing_pipeline_field_returns_error() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            ..Default::default()
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("pipeline"));
    }

    #[test]
    fn empty_pipeline_array_returns_error() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            pipeline: Some(vec![]),
            ..Default::default()
        };
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── step id validation ───────────────────────────────────────────────────

    #[test]
    fn step_missing_id_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            prompt: Some("hello".to_string()),
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        }]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    // ── step body types ──────────────────────────────────────────────────────

    #[test]
    fn skill_step_round_trips() {
        // skill: steps are now supported (SPEC §6).
        let dto = minimal_dto(vec![StepDto {
            id: Some("sk".to_string()),
            skill: Some("ail/code_review".to_string()),
            ..Default::default()
        }]);
        let pipeline = validate(dto, source()).expect("skill: step should be accepted");
        assert!(matches!(pipeline.steps[0].body, StepBody::Skill { .. }));
        if let StepBody::Skill { ref name } = pipeline.steps[0].body {
            assert_eq!(name, "ail/code_review");
        }
    }

    #[test]
    fn skill_step_empty_name_returns_error() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("sk".to_string()),
            skill: Some("  ".to_string()),
            ..Default::default()
        }]);
        let err = validate(dto, source()).expect_err("empty skill name should fail");
        assert_eq!(
            err.error_type(),
            error_types::CONFIG_VALIDATION_FAILED,
            "empty skill name must yield CONFIG_VALIDATION_FAILED"
        );
        assert!(
            err.detail().contains("empty"),
            "error detail should mention 'empty', got: {}",
            err.detail()
        );
    }

    #[test]
    fn sub_pipeline_step_round_trips() {
        let dto = minimal_dto(vec![StepDto {
            id: Some("sub".to_string()),
            pipeline: Some("child.ail.yaml".to_string()),
            ..Default::default()
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
            message: Some("Waiting for approval".to_string()),
            ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
            }),
            ..Default::default()
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
            context: Some(ContextDto::default()),
            ..Default::default()
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

    // ── condition expression parsing ────────────────────────────────────────

    #[test]
    fn condition_eq_expression_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.test.exit_code }} == 0".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        assert!(
            matches!(cond, Condition::Expression(expr) if expr.op == crate::config::domain::ConditionOp::Eq),
            "Expected Expression(Eq), got: {cond:?}"
        );
    }

    #[test]
    fn condition_ne_expression_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.test.exit_code }} != 0".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        assert!(
            matches!(cond, Condition::Expression(expr) if expr.op == crate::config::domain::ConditionOp::Ne),
        );
    }

    #[test]
    fn condition_contains_expression_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.review.response }} contains 'LGTM'".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        match cond {
            Condition::Expression(expr) => {
                assert_eq!(expr.op, crate::config::domain::ConditionOp::Contains);
                assert_eq!(expr.rhs, "LGTM");
            }
            other => panic!("Expected Expression(Contains), got: {other:?}"),
        }
    }

    #[test]
    fn condition_starts_with_expression_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.check.response }} starts_with 'PASS'".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        assert!(matches!(
            cond,
            Condition::Expression(expr) if expr.op == crate::config::domain::ConditionOp::StartsWith
        ));
    }

    #[test]
    fn condition_ends_with_expression_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.check.response }} ends_with 'done'".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        assert!(matches!(
            cond,
            Condition::Expression(expr) if expr.op == crate::config::domain::ConditionOp::EndsWith
        ));
    }

    #[test]
    fn condition_expression_empty_lhs_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some(" == 0".to_string());
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("empty left-hand side"));
    }

    #[test]
    fn condition_expression_empty_rhs_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.test.exit_code }} == ".to_string());
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("empty right-hand side"));
    }

    #[test]
    fn condition_expression_strips_quotes_from_rhs() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.review.response }} contains 'text value'".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        match cond {
            Condition::Expression(expr) => {
                assert_eq!(expr.rhs, "text value");
            }
            other => panic!("Expected Expression, got: {other:?}"),
        }
    }

    #[test]
    fn condition_expression_double_quotes_on_rhs() {
        let mut step = minimal_step("s", "hello");
        step.condition = Some("{{ step.review.response }} contains \"LGTM\"".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        let cond = pipeline.steps[0]
            .condition
            .as_ref()
            .expect("should have condition");
        match cond {
            Condition::Expression(expr) => {
                assert_eq!(expr.rhs, "LGTM");
            }
            other => panic!("Expected Expression, got: {other:?}"),
        }
    }

    // ── on_result branches ───────────────────────────────────────────────────

    #[test]
    fn on_result_contains_continue_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_result = Some(vec![OnResultBranchDto {
            contains: Some("SUCCESS".to_string()),
            action: Some("continue".to_string()),
            ..Default::default()
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
            exit_code: Some(ExitCodeDto::Integer(1)),
            action: Some("break".to_string()),
            ..Default::default()
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
            exit_code: Some(ExitCodeDto::Keyword("any".to_string())),
            action: Some("abort_pipeline".to_string()),
            ..Default::default()
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
            exit_code: Some(ExitCodeDto::Keyword("none".to_string())),
            action: Some("continue".to_string()),
            ..Default::default()
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
            always: Some(true),
            action: Some("pause_for_human".to_string()),
            ..Default::default()
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
            action: Some("pipeline: child.ail.yaml".to_string()),
            ..Default::default()
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
            action: Some("pipeline:".to_string()),
            ..Default::default()
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
            action: Some("teleport".to_string()),
            ..Default::default()
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
            ..Default::default()
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
            action: Some("continue".to_string()),
            ..Default::default()
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
            action: Some("continue".to_string()),
            ..Default::default()
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
                ..Default::default()
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
            ..Default::default()
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
                    ..Default::default()
                }),
                ..Default::default()
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
            ..Default::default()
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
                ..Default::default()
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
            ..Default::default()
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.defaults.model.as_deref(), Some("fallback-model"));
    }

    #[test]
    fn timeout_seconds_propagates() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                timeout_seconds: Some(120),
                ..Default::default()
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
            ..Default::default()
        };
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(pipeline.timeout_seconds, Some(120));
    }

    #[test]
    fn default_tools_propagate() {
        let dto = PipelineFileDto {
            version: Some("1".to_string()),
            defaults: Some(DefaultsDto {
                tools: Some(ToolsDto {
                    disabled: false,
                    allow: vec!["Bash".to_string()],
                    deny: vec![],
                }),
                ..Default::default()
            }),
            pipeline: Some(vec![minimal_step("s", "hello")]),
            ..Default::default()
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

    // ── modify_output action ────────────────────────────────────────────────

    #[test]
    fn action_modify_output_round_trips_with_skip_default() {
        let mut step = minimal_step("gate", "unused");
        step.prompt = None;
        step.action = Some("modify_output".to_string());
        step.message = Some("Please review".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(matches!(
            pipeline.steps[0].body,
            StepBody::Action(ActionKind::ModifyOutput {
                headless_behavior: crate::config::domain::HitlHeadlessBehavior::Skip,
                ..
            })
        ));
    }

    #[test]
    fn action_modify_output_abort_headless_round_trips() {
        let mut step = minimal_step("gate", "unused");
        step.prompt = None;
        step.action = Some("modify_output".to_string());
        step.on_headless = Some("abort".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(matches!(
            pipeline.steps[0].body,
            StepBody::Action(ActionKind::ModifyOutput {
                headless_behavior: crate::config::domain::HitlHeadlessBehavior::Abort,
                ..
            })
        ));
    }

    #[test]
    fn action_modify_output_use_default_requires_default_value() {
        let mut step = minimal_step("gate", "unused");
        step.prompt = None;
        step.action = Some("modify_output".to_string());
        step.on_headless = Some("use_default".to_string());
        // default_value is None → validation should fail
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("default_value"));
    }

    #[test]
    fn action_modify_output_use_default_with_value_round_trips() {
        let mut step = minimal_step("gate", "unused");
        step.prompt = None;
        step.action = Some("modify_output".to_string());
        step.on_headless = Some("use_default".to_string());
        step.default_value = Some("fallback text".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        if let StepBody::Action(ActionKind::ModifyOutput {
            headless_behavior,
            default_value,
        }) = &pipeline.steps[0].body
        {
            assert_eq!(
                *headless_behavior,
                crate::config::domain::HitlHeadlessBehavior::UseDefault
            );
            assert_eq!(default_value.as_deref(), Some("fallback text"));
        } else {
            panic!("expected ModifyOutput step body");
        }
    }

    #[test]
    fn action_modify_output_unknown_on_headless_returns_error() {
        let mut step = minimal_step("gate", "unused");
        step.prompt = None;
        step.action = Some("modify_output".to_string());
        step.on_headless = Some("magic".to_string());
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("magic"));
    }

    // ── on_error field ──────────────────────────────────────────────────────

    #[test]
    fn on_error_defaults_to_none() {
        let dto = minimal_dto(vec![minimal_step("s", "hello")]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert!(pipeline.steps[0].on_error.is_none());
    }

    #[test]
    fn on_error_continue_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("continue".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(
            pipeline.steps[0].on_error,
            Some(crate::config::domain::OnError::Continue)
        );
    }

    #[test]
    fn on_error_abort_pipeline_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("abort_pipeline".to_string());
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(
            pipeline.steps[0].on_error,
            Some(crate::config::domain::OnError::AbortPipeline)
        );
    }

    #[test]
    fn on_error_retry_with_max_retries_round_trips() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("retry".to_string());
        step.max_retries = Some(3);
        let dto = minimal_dto(vec![step]);
        let pipeline = validate(dto, source()).expect("should succeed");
        assert_eq!(
            pipeline.steps[0].on_error,
            Some(crate::config::domain::OnError::Retry { max_retries: 3 })
        );
    }

    #[test]
    fn on_error_retry_without_max_retries_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("retry".to_string());
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("max_retries"));
    }

    #[test]
    fn on_error_retry_with_zero_max_retries_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("retry".to_string());
        step.max_retries = Some(0);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("max_retries"));
    }

    #[test]
    fn max_retries_without_on_error_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.max_retries = Some(3);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("max_retries"));
    }

    #[test]
    fn max_retries_with_continue_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("continue".to_string());
        step.max_retries = Some(3);
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    }

    #[test]
    fn unknown_on_error_value_returns_error() {
        let mut step = minimal_step("s", "hello");
        step.on_error = Some("panic".to_string());
        let dto = minimal_dto(vec![step]);
        let err = validate(dto, source()).expect_err("should fail");
        assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
        assert!(err.detail().contains("panic"));
    }
}
