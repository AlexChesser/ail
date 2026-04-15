//! on_result branch evaluation and tool policy construction.

#![allow(clippy::result_large_err)]

use crate::config::domain::{ExitCodeMatch, ResultAction, ResultMatcher};
use crate::error::AilError;
use crate::runner::ToolPermissionPolicy;
use crate::session::{Session, TurnEntry};

use super::condition::evaluate_condition;

/// Evaluate `on_result` branches against the most recent `TurnEntry`.
/// Returns the action of the first matching branch, or `None` if no branch matches.
///
/// `validated_input` is the parsed JSON from `input_schema` validation (SPEC §26.2).
/// When present, `ResultMatcher::Field` can match against named fields in the input JSON.
///
/// An `Err` return indicates the `expression:` matcher failed to evaluate —
/// typically an unresolvable template variable in the expression's LHS
/// (SPEC §11 contract). The pipeline aborts rather than silently treating the
/// branch as non-matching.
pub(in crate::executor) fn evaluate_on_result(
    branches: &[crate::config::domain::ResultBranch],
    session: &Session,
    step_id: &str,
    entry: &TurnEntry,
    validated_input: Option<&serde_json::Value>,
) -> Result<Option<ResultAction>, AilError> {
    for branch in branches {
        let matched = match &branch.matcher {
            ResultMatcher::Contains(text) => {
                let haystack = entry
                    .response
                    .as_deref()
                    .or(entry.stdout.as_deref())
                    .unwrap_or("");
                let haystack_lower = haystack.to_lowercase();
                haystack_lower.contains(&text.to_lowercase())
            }
            ResultMatcher::ExitCode(ExitCodeMatch::Exact(n)) => entry.exit_code == Some(*n),
            ResultMatcher::ExitCode(ExitCodeMatch::Any) => {
                // `any` matches any non-zero exit code — does NOT match 0.
                matches!(entry.exit_code, Some(c) if c != 0)
            }
            ResultMatcher::Field { name, equals } => {
                // Match against a named field in the validated input JSON (SPEC §26.4).
                validated_input
                    .and_then(|json| json.get(name))
                    .is_some_and(|val| val == equals)
            }
            ResultMatcher::Expression { condition, .. } => {
                // SPEC §5.4 `expression:` matcher — delegate to the shared
                // condition evaluator so `condition:` and `expression:` stay in
                // sync grammatically and semantically.
                evaluate_condition(condition, session, step_id)?
            }
            ResultMatcher::Always => true,
        };

        if matched {
            return Ok(Some(branch.action.clone()));
        }
    }
    Ok(None)
}

/// Build a `ToolPermissionPolicy` from an optional `ToolPolicy` domain value.
pub(in crate::executor) fn build_tool_policy(
    tools: Option<&crate::config::domain::ToolPolicy>,
) -> ToolPermissionPolicy {
    match tools {
        Some(t) if t.disabled => ToolPermissionPolicy::NoTools,
        Some(t) if !t.allow.is_empty() && !t.deny.is_empty() => ToolPermissionPolicy::Mixed {
            allow: t.allow.clone(),
            deny: t.deny.clone(),
        },
        Some(t) if !t.allow.is_empty() => ToolPermissionPolicy::Allowlist(t.allow.clone()),
        Some(t) if !t.deny.is_empty() => ToolPermissionPolicy::Denylist(t.deny.clone()),
        _ => ToolPermissionPolicy::RunnerDefault,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::ToolPolicy;
    use crate::runner::ToolPermissionPolicy;

    // ── build_tool_policy ────────────────────────────────────────────────────

    fn make_policy(disabled: bool, allow: Vec<&str>, deny: Vec<&str>) -> ToolPolicy {
        ToolPolicy {
            disabled,
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn build_tool_policy_none_returns_runner_default() {
        assert!(matches!(
            build_tool_policy(None),
            ToolPermissionPolicy::RunnerDefault
        ));
    }

    #[test]
    fn build_tool_policy_disabled_returns_no_tools() {
        let policy = make_policy(true, vec![], vec![]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::NoTools
        ));
    }

    #[test]
    fn build_tool_policy_allow_and_deny_returns_mixed() {
        let policy = make_policy(false, vec!["Bash"], vec!["Write"]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::Mixed { .. }
        ));
    }

    #[test]
    fn build_tool_policy_allow_only_returns_allowlist() {
        let policy = make_policy(false, vec!["Read", "Bash"], vec![]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::Allowlist(_)
        ));
    }

    #[test]
    fn build_tool_policy_deny_only_returns_denylist() {
        let policy = make_policy(false, vec![], vec!["Bash"]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::Denylist(_)
        ));
    }

    #[test]
    fn build_tool_policy_empty_returns_runner_default() {
        let policy = make_policy(false, vec![], vec![]);
        assert!(matches!(
            build_tool_policy(Some(&policy)),
            ToolPermissionPolicy::RunnerDefault
        ));
    }
}
