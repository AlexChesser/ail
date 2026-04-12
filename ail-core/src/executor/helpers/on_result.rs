//! on_result branch evaluation and tool policy construction.

use crate::config::domain::{ExitCodeMatch, ResultAction, ResultMatcher};
use crate::runner::ToolPermissionPolicy;
use crate::session::TurnEntry;

/// Evaluate `on_result` branches against the most recent `TurnEntry`.
/// Returns the action of the first matching branch, or `None` if no branch matches.
pub(in crate::executor) fn evaluate_on_result(
    branches: &[crate::config::domain::ResultBranch],
    entry: &TurnEntry,
) -> Option<ResultAction> {
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
            ResultMatcher::Always => true,
        };

        if matched {
            return Some(branch.action.clone());
        }
    }
    None
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
