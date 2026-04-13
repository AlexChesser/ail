//! Condition expression evaluation at runtime (SPEC §12).
//!
//! Named conditions (`always`, `never`) are handled statically at parse time.
//! Expression conditions are evaluated here against the live session state —
//! template variables are resolved first, then the operator is applied.

#![allow(clippy::result_large_err)]

use crate::config::domain::{Condition, ConditionExpr, ConditionOp};
use crate::error::AilError;
use crate::session::Session;
use crate::template;

/// Evaluate a [`Condition`] against the current session state.
///
/// Returns `true` if the step should execute, `false` if it should be skipped.
/// `Condition::Always` → `true`, `Condition::Never` → `false`.
/// `Condition::Expression` → resolve templates, then evaluate the operator.
///
/// Template resolution failures produce a `ConditionInvalid` error (not a
/// `TemplateUnresolved` error) to distinguish from prompt-template failures.
pub fn evaluate_condition(
    condition: &Condition,
    session: &Session,
    step_id: &str,
) -> Result<bool, AilError> {
    match condition {
        Condition::Always => Ok(true),
        Condition::Never => Ok(false),
        Condition::Expression(expr) => evaluate_expression(expr, session, step_id),
    }
}

fn evaluate_expression(
    expr: &ConditionExpr,
    session: &Session,
    step_id: &str,
) -> Result<bool, AilError> {
    let lhs_resolved =
        template::resolve(&expr.lhs, session).map_err(|e| AilError::ConditionInvalid {
            detail: format!(
                "Step '{step_id}' condition: failed to resolve left-hand side '{}': {}",
                expr.lhs,
                e.detail()
            ),
            context: None,
        })?;

    let rhs_resolved =
        template::resolve(&expr.rhs, session).map_err(|e| AilError::ConditionInvalid {
            detail: format!(
                "Step '{step_id}' condition: failed to resolve right-hand side '{}': {}",
                expr.rhs,
                e.detail()
            ),
            context: None,
        })?;

    let lhs = lhs_resolved.trim();
    let rhs = rhs_resolved.trim();

    let result = match &expr.op {
        ConditionOp::Eq => lhs == rhs,
        ConditionOp::Ne => lhs != rhs,
        ConditionOp::Contains => lhs.to_lowercase().contains(&rhs.to_lowercase()),
        ConditionOp::StartsWith => lhs.to_lowercase().starts_with(&rhs.to_lowercase()),
        ConditionOp::EndsWith => lhs.to_lowercase().ends_with(&rhs.to_lowercase()),
    };

    tracing::debug!(
        step_id = %step_id,
        lhs = %lhs,
        op = ?expr.op,
        rhs = %rhs,
        result = %result,
        "condition expression evaluated"
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::{ConditionExpr, ConditionOp, Step, StepBody, StepId};
    use crate::session::TurnEntry;
    use crate::test_helpers::make_session;
    use std::time::SystemTime;

    fn session_with_shell_entry(step_id: &str, exit_code: i32, stdout: &str) -> Session {
        let mut session = make_session(vec![prompt_step("dummy", "dummy")]);
        session.turn_log.append(TurnEntry {
            step_id: step_id.to_string(),
            prompt: "cmd".to_string(),
            stdout: Some(stdout.to_string()),
            stderr: Some(String::new()),
            exit_code: Some(exit_code),
            ..Default::default()
        });
        session
    }

    fn session_with_prompt_entry(step_id: &str, response: &str) -> Session {
        let mut session = make_session(vec![prompt_step("dummy", "dummy")]);
        session.turn_log.append(TurnEntry {
            step_id: step_id.to_string(),
            prompt: "ask".to_string(),
            response: Some(response.to_string()),
            ..Default::default()
        });
        session
    }

    fn prompt_step(id: &str, text: &str) -> Step {
        Step {
            id: StepId(id.to_string()),
            body: StepBody::Prompt(text.to_string()),
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
            input_schema: None,
        }
    }

    #[test]
    fn always_returns_true() {
        let session = make_session(vec![]);
        assert!(evaluate_condition(&Condition::Always, &session, "s").unwrap());
    }

    #[test]
    fn never_returns_false() {
        let session = make_session(vec![]);
        assert!(!evaluate_condition(&Condition::Never, &session, "s").unwrap());
    }

    #[test]
    fn eq_operator_matches_exit_code() {
        let session = session_with_shell_entry("test", 0, "ok");
        let expr = ConditionExpr {
            lhs: "{{ step.test.exit_code }}".to_string(),
            op: ConditionOp::Eq,
            rhs: "0".to_string(),
        };
        assert!(evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn eq_operator_does_not_match_different_exit_code() {
        let session = session_with_shell_entry("test", 1, "fail");
        let expr = ConditionExpr {
            lhs: "{{ step.test.exit_code }}".to_string(),
            op: ConditionOp::Eq,
            rhs: "0".to_string(),
        };
        assert!(!evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn ne_operator_works() {
        let session = session_with_shell_entry("test", 1, "fail");
        let expr = ConditionExpr {
            lhs: "{{ step.test.exit_code }}".to_string(),
            op: ConditionOp::Ne,
            rhs: "0".to_string(),
        };
        assert!(evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn contains_operator_case_insensitive() {
        let session = session_with_prompt_entry("review", "Everything looks LGTM to me");
        let expr = ConditionExpr {
            lhs: "{{ step.review.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "lgtm".to_string(),
        };
        assert!(evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn contains_operator_no_match() {
        let session = session_with_prompt_entry("review", "Needs more work");
        let expr = ConditionExpr {
            lhs: "{{ step.review.response }}".to_string(),
            op: ConditionOp::Contains,
            rhs: "LGTM".to_string(),
        };
        assert!(!evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn starts_with_operator() {
        let session = session_with_prompt_entry("check", "PASS: all tests passed");
        let expr = ConditionExpr {
            lhs: "{{ step.check.response }}".to_string(),
            op: ConditionOp::StartsWith,
            rhs: "PASS".to_string(),
        };
        assert!(evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn ends_with_operator() {
        let session = session_with_prompt_entry("check", "All tests passed");
        let expr = ConditionExpr {
            lhs: "{{ step.check.response }}".to_string(),
            op: ConditionOp::EndsWith,
            rhs: "passed".to_string(),
        };
        assert!(evaluate_condition(&Condition::Expression(expr), &session, "s").unwrap());
    }

    #[test]
    fn unresolvable_template_produces_condition_invalid() {
        let session = make_session(vec![]);
        let expr = ConditionExpr {
            lhs: "{{ step.nonexistent.response }}".to_string(),
            op: ConditionOp::Eq,
            rhs: "value".to_string(),
        };
        let err =
            evaluate_condition(&Condition::Expression(expr), &session, "mycheck").unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CONDITION_INVALID
        );
        assert!(err.detail().contains("mycheck"));
    }
}
