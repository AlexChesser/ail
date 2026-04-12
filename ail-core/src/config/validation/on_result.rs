//! on_result branch parsing — DTO to domain conversion for result matchers and actions.

#![allow(clippy::result_large_err)]

use crate::config::domain::{ExitCodeMatch, ResultAction, ResultBranch, ResultMatcher};
use crate::config::dto::ExitCodeDto;
use crate::error::AilError;

use super::cfg_err;

pub(in crate::config) fn parse_result_branches(
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
