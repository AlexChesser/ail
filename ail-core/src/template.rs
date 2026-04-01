#![allow(clippy::result_large_err)]

use crate::error::{error_types, AilError};
use crate::session::Session;

/// Resolve `{{ variable }}` template syntax in `template` against `session` state.
///
/// All recognised variables are listed in SPEC §11.
/// Unrecognised syntax and missing values are errors — silent empty is never permitted.
pub fn resolve(template: &str, session: &Session) -> Result<String, AilError> {
    let mut output = String::with_capacity(template.len());
    let mut remaining = template;

    while let Some(start) = remaining.find("{{") {
        // Text before the opening `{{`
        output.push_str(&remaining[..start]);
        remaining = &remaining[start + 2..];

        let end = remaining.find("}}").ok_or_else(|| AilError {
            error_type: error_types::TEMPLATE_UNRESOLVED,
            title: "Unclosed template variable",
            detail: "Found '{{' without matching '}}'".to_string(),
            context: None,
        })?;

        let variable = remaining[..end].trim();
        remaining = &remaining[end + 2..];

        let value = resolve_variable(variable, session)?;
        output.push_str(&value);
    }

    output.push_str(remaining);
    Ok(output)
}

fn resolve_variable(variable: &str, session: &Session) -> Result<String, AilError> {
    match variable {
        "session.invocation_prompt" | "step.invocation.prompt" => {
            Ok(session.invocation_prompt.clone())
        }

        "step.invocation.response" => session
            .turn_log
            .response_for_step("invocation")
            .map(|s| s.to_string())
            .ok_or_else(|| AilError {
                error_type: error_types::TEMPLATE_UNRESOLVED,
                title: "Step response not found",
                detail: "No response recorded for step 'invocation'".to_string(),
                context: None,
            }),

        "last_response" => session
            .turn_log
            .last_response()
            .map(|s| s.to_string())
            .ok_or_else(|| AilError {
                error_type: error_types::TEMPLATE_UNRESOLVED,
                title: "No last response",
                detail: "No responses have been recorded yet".to_string(),
                context: None,
            }),

        "session.tool" => Ok("claude".to_string()),

        "session.cwd" => std::env::current_dir()
            .map(|p| p.display().to_string())
            .map_err(|e| AilError {
                error_type: error_types::TEMPLATE_UNRESOLVED,
                title: "Cannot determine working directory",
                detail: e.to_string(),
                context: None,
            }),

        "pipeline.run_id" => Ok(session.run_id.clone()),

        other => {
            // Handle `step.<id>.response`, `step.<id>.result`, `step.<id>.stdout`,
            // `step.<id>.stderr`, `step.<id>.exit_code`, and `env.<VAR_NAME>`.
            if let Some(step_id) = other
                .strip_prefix("step.")
                .and_then(|s| s.strip_suffix(".response"))
            {
                return session
                    .turn_log
                    .response_for_step(step_id)
                    .map(|s| s.to_string())
                    .ok_or_else(|| AilError {
                        error_type: error_types::TEMPLATE_UNRESOLVED,
                        title: "Step response not found",
                        detail: format!("No response recorded for step '{step_id}'"),
                        context: None,
                    });
            }

            if let Some(step_id) = other
                .strip_prefix("step.")
                .and_then(|s| s.strip_suffix(".result"))
            {
                return session
                    .turn_log
                    .result_for_step(step_id)
                    .ok_or_else(|| AilError {
                        error_type: error_types::TEMPLATE_UNRESOLVED,
                        title: "Step result not found",
                        detail: format!("No result recorded for step '{step_id}'"),
                        context: None,
                    });
            }

            if let Some(step_id) = other
                .strip_prefix("step.")
                .and_then(|s| s.strip_suffix(".stdout"))
            {
                return session
                    .turn_log
                    .stdout_for_step(step_id)
                    .map(|s| s.to_string())
                    .ok_or_else(|| AilError {
                        error_type: error_types::TEMPLATE_UNRESOLVED,
                        title: "Step stdout not found",
                        detail: format!("No stdout recorded for step '{step_id}'"),
                        context: None,
                    });
            }

            if let Some(step_id) = other
                .strip_prefix("step.")
                .and_then(|s| s.strip_suffix(".stderr"))
            {
                return session
                    .turn_log
                    .stderr_for_step(step_id)
                    .map(|s| s.to_string())
                    .ok_or_else(|| AilError {
                        error_type: error_types::TEMPLATE_UNRESOLVED,
                        title: "Step stderr not found",
                        detail: format!("No stderr recorded for step '{step_id}'"),
                        context: None,
                    });
            }

            if let Some(step_id) = other
                .strip_prefix("step.")
                .and_then(|s| s.strip_suffix(".exit_code"))
            {
                return session
                    .turn_log
                    .exit_code_for_step(step_id)
                    .map(|c| c.to_string())
                    .ok_or_else(|| AilError {
                        error_type: error_types::TEMPLATE_UNRESOLVED,
                        title: "Step exit_code not found",
                        detail: format!("No exit_code recorded for step '{step_id}'"),
                        context: None,
                    });
            }

            if let Some(var_name) = other.strip_prefix("env.") {
                return std::env::var(var_name).map_err(|_| AilError {
                    error_type: error_types::TEMPLATE_UNRESOLVED,
                    title: "Environment variable not set",
                    detail: format!("Environment variable '{var_name}' is not set"),
                    context: None,
                });
            }

            Err(AilError {
                error_type: error_types::TEMPLATE_UNRESOLVED,
                title: "Unrecognised template variable",
                detail: format!("'{{ {other} }}' is not a recognised template variable"),
                context: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::domain::Pipeline;
    use crate::session::{Session, TurnEntry};
    use std::time::SystemTime;

    fn make_session() -> Session {
        Session::new(Pipeline::passthrough(), "original prompt".to_string())
    }

    #[allow(dead_code)]
    fn append_response(session: &mut Session, step_id: &str, response: &str) {
        session.turn_log.append(TurnEntry {
            step_id: step_id.to_string(),
            prompt: "p".to_string(),
            response: Some(response.to_string()),
            timestamp: SystemTime::now(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            runner_session_id: None,
            stdout: None,
            stderr: None,
            exit_code: None,
        });
    }

    #[allow(dead_code)]
    fn append_context(session: &mut Session, step_id: &str, stdout: &str, stderr: &str, code: i32) {
        session.turn_log.append(TurnEntry {
            step_id: step_id.to_string(),
            prompt: "cmd".to_string(),
            response: None,
            timestamp: SystemTime::now(),
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            runner_session_id: None,
            stdout: Some(stdout.to_string()),
            stderr: Some(stderr.to_string()),
            exit_code: Some(code),
        });
    }

    #[test]
    fn no_variables_unchanged() {
        let session = make_session();
        let result = resolve("no variables here", &session).unwrap();
        assert_eq!(result, "no variables here");
    }

    #[test]
    fn invocation_prompt_resolves() {
        let session = make_session();
        let result = resolve("{{ step.invocation.prompt }}", &session).unwrap();
        assert_eq!(result, "original prompt");
    }

    #[test]
    fn pipeline_run_id_resolves() {
        let session = make_session();
        let result = resolve("{{ pipeline.run_id }}", &session).unwrap();
        assert_eq!(result, session.run_id);
    }

    #[test]
    fn session_tool_resolves_to_claude() {
        let session = make_session();
        let result = resolve("{{ session.tool }}", &session).unwrap();
        assert_eq!(result, "claude");
    }
}
