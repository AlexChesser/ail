#![allow(clippy::result_large_err)]

use crate::error::AilError;
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

        let end = remaining
            .find("}}")
            .ok_or_else(|| unresolved("Found '{{' without matching '}}'"))?;

        let variable = remaining[..end].trim();
        remaining = &remaining[end + 2..];

        let value = resolve_variable(variable, session)?;
        output.push_str(&value);
    }

    output.push_str(remaining);
    Ok(output)
}

fn unresolved(detail: impl Into<String>) -> AilError {
    AilError::TemplateUnresolved {
        detail: detail.into(),
        context: None,
    }
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
            .ok_or_else(|| unresolved("No response recorded for step 'invocation'")),

        "last_response" => session
            .turn_log
            .last_response()
            .map(|s| s.to_string())
            .ok_or_else(|| unresolved("No responses have been recorded yet")),

        "session.tool" => Ok(session.runner_name.clone()),

        "session.cwd" => Ok(session.cwd.clone()),

        "pipeline.run_id" => Ok(session.run_id.clone()),

        other => {
            // `step.<id>.<field>` — split on last dot to get step_id and field.
            if let Some(rest) = other.strip_prefix("step.") {
                if let Some(dot) = rest.rfind('.') {
                    let step_id = &rest[..dot];
                    let field = &rest[dot + 1..];
                    return match field {
                        "response" => session
                            .turn_log
                            .response_for_step(step_id)
                            .map(|s| s.to_string())
                            .ok_or_else(|| {
                                unresolved(format!("No response recorded for step '{step_id}'"))
                            }),
                        "result" => session.turn_log.result_for_step(step_id).ok_or_else(|| {
                            unresolved(format!("No result recorded for step '{step_id}'"))
                        }),
                        "stdout" => session
                            .turn_log
                            .stdout_for_step(step_id)
                            .map(|s| s.to_string())
                            .ok_or_else(|| {
                                unresolved(format!("No stdout recorded for step '{step_id}'"))
                            }),
                        "stderr" => session
                            .turn_log
                            .stderr_for_step(step_id)
                            .map(|s| s.to_string())
                            .ok_or_else(|| {
                                unresolved(format!("No stderr recorded for step '{step_id}'"))
                            }),
                        "exit_code" => session
                            .turn_log
                            .exit_code_for_step(step_id)
                            .map(|c| c.to_string())
                            .ok_or_else(|| {
                                unresolved(format!("No exit_code recorded for step '{step_id}'"))
                            }),
                        "tool_calls" => {
                            let events = session
                                .turn_log
                                .tool_events_for_step(step_id)
                                .ok_or_else(|| {
                                    unresolved(format!("No entry recorded for step '{step_id}'"))
                                })?;
                            serde_json::to_string(events).map_err(|e| {
                                unresolved(format!(
                                    "Failed to serialize tool_calls for step '{step_id}': {e}"
                                ))
                            })
                        }
                        _ => Err(unresolved(format!(
                            "'{{ {variable} }}' is not a recognised template variable"
                        ))),
                    };
                }
            }

            if let Some(var_name) = other.strip_prefix("env.") {
                return std::env::var(var_name).map_err(|_| {
                    unresolved(format!("Environment variable '{var_name}' is not set"))
                });
            }

            Err(unresolved(format!(
                "'{{ {other} }}' is not a recognised template variable"
            )))
        }
    }
}
