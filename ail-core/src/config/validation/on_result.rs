//! on_result branch parsing — DTO to domain conversion for result matchers and actions.

#![allow(clippy::result_large_err)]

use crate::config::domain::{ExitCodeMatch, ResultAction, ResultBranch, ResultMatcher};
use crate::config::dto::{ExitCodeDto, FieldEqualsActionDto, OnResultDto};
use crate::error::AilError;

use super::{cfg_err, parse_condition_expression};

/// Parse `on_result` from either the multi-branch array format or the
/// field-equals binary branch format (SPEC §5.4, §26.4).
pub(in crate::config) fn parse_on_result(
    step_id: &str,
    dto: OnResultDto,
    input_schema: Option<&serde_json::Value>,
) -> Result<Vec<ResultBranch>, AilError> {
    match dto {
        OnResultDto::Branches(branches) => parse_result_branches(step_id, branches),
        OnResultDto::FieldEquals(fe) => parse_field_equals(step_id, *fe, input_schema),
    }
}

fn parse_result_branches(
    step_id: &str,
    branches: Vec<crate::config::dto::OnResultBranchDto>,
) -> Result<Vec<ResultBranch>, AilError> {
    branches
        .into_iter()
        .enumerate()
        .map(|(i, branch)| {
            let matcher_count = [
                branch.contains.is_some(),
                branch.exit_code.is_some(),
                branch.matches.is_some(),
                branch.expression.is_some(),
                branch.always.is_some(),
            ]
            .iter()
            .filter(|&&b| b)
            .count();

            if matcher_count != 1 {
                return Err(cfg_err!(
                    "Step '{step_id}' on_result branch {i} must have exactly one matcher \
                     (contains, exit_code, matches, expression, always); found {matcher_count}"
                ));
            }

            let action_str = branch.action.ok_or_else(|| {
                cfg_err!("Step '{step_id}' on_result branch {i} must declare an 'action'")
            })?;

            let action = parse_action_string(&action_str, branch.prompt, step_id, i)?;

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
            } else if let Some(regex_literal) = branch.matches {
                // Named `matches:` is shorthand for `expression: '{{ step.<id>.response }} matches /.../'`
                // (SPEC §5.4). Desugar at parse time so both forms share a single evaluator.
                let source = format!("{{{{ step.{step_id}.response }}}} matches {regex_literal}");
                let condition = parse_condition_expression(&source, step_id).map_err(|e| {
                    cfg_err!(
                        "Step '{step_id}' on_result branch {i} matches: {}",
                        e.detail()
                    )
                })?;
                ResultMatcher::Expression { source, condition }
            } else if let Some(expr_str) = branch.expression {
                let condition = parse_condition_expression(&expr_str, step_id).map_err(|e| {
                    cfg_err!(
                        "Step '{step_id}' on_result branch {i} expression: {}",
                        e.detail()
                    )
                })?;
                ResultMatcher::Expression {
                    source: expr_str,
                    condition,
                }
            } else {
                ResultMatcher::Always
            };

            Ok(ResultBranch { matcher, action })
        })
        .collect()
}

/// Parse a `field:` + `equals:` binary branch into two `ResultBranch`es:
/// one for `if_true` (Field matcher) and one for `if_false` (Always matcher).
fn parse_field_equals(
    step_id: &str,
    fe: crate::config::dto::FieldEqualsDto,
    input_schema: Option<&serde_json::Value>,
) -> Result<Vec<ResultBranch>, AilError> {
    // Parse-time rule: field+equals requires input_schema with the referenced field (SPEC §26.4).
    let schema = input_schema.ok_or_else(|| {
        cfg_err!(
            "Step '{step_id}' uses on_result field: + equals: but does not declare an \
             input_schema — field: + equals: requires input_schema (SPEC §26.4)"
        )
    })?;

    // Verify the schema declares the referenced field.
    validate_field_in_schema(&fe.field, schema, step_id)?;

    let if_true_action = parse_field_equals_action(&fe.if_true, step_id, "if_true")?;
    let mut branches = vec![ResultBranch {
        matcher: ResultMatcher::Field {
            name: fe.field,
            equals: fe.equals,
        },
        action: if_true_action,
    }];

    if let Some(if_false) = fe.if_false {
        let if_false_action = parse_field_equals_action(&if_false, step_id, "if_false")?;
        branches.push(ResultBranch {
            matcher: ResultMatcher::Always,
            action: if_false_action,
        });
    }

    Ok(branches)
}

/// Verify that a field name exists in the input_schema's `properties` or `required` arrays.
fn validate_field_in_schema(
    field_name: &str,
    schema: &serde_json::Value,
    step_id: &str,
) -> Result<(), AilError> {
    let has_in_properties = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .is_some_and(|props| props.contains_key(field_name));

    let has_in_required = schema
        .get("required")
        .and_then(|r| r.as_array())
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(field_name)));

    if !has_in_properties && !has_in_required {
        return Err(cfg_err!(
            "Step '{step_id}' on_result references field '{field_name}' but the step's \
             input_schema does not include this field in 'properties' or 'required'"
        ));
    }
    Ok(())
}

/// Parse an action from a `FieldEqualsActionDto`.
fn parse_field_equals_action(
    action_dto: &FieldEqualsActionDto,
    step_id: &str,
    branch_label: &str,
) -> Result<ResultAction, AilError> {
    parse_action_string(
        &action_dto.action,
        action_dto.prompt.clone(),
        step_id,
        branch_label,
    )
}

/// Parse an action string into a `ResultAction`. Used by both the multi-branch
/// and field-equals formats.
fn parse_action_string<D: std::fmt::Display>(
    action_str: &str,
    prompt: Option<String>,
    step_id: &str,
    branch_label: D,
) -> Result<ResultAction, AilError> {
    if let Some(path) = action_str.strip_prefix("pipeline:").map(str::trim) {
        if path.is_empty() {
            return Err(cfg_err!(
                "Step '{step_id}' on_result branch {branch_label} action 'pipeline:' requires a path"
            ));
        }
        Ok(ResultAction::Pipeline {
            path: path.to_string(),
            prompt,
        })
    } else {
        match action_str {
            "continue" => Ok(ResultAction::Continue),
            "break" => Ok(ResultAction::Break),
            "abort_pipeline" => Ok(ResultAction::AbortPipeline),
            "pause_for_human" => Ok(ResultAction::PauseForHuman),
            other => Err(cfg_err!(
                "Step '{step_id}' on_result branch {branch_label} specifies unknown action '{other}'"
            )),
        }
    }
}
