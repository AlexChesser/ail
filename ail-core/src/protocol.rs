//! NDJSON stdin control protocol — message types shared by `--once --output-format json`
//! and `chat` modes.
//!
//! # Format
//!
//! Each line on stdin is either:
//! - A JSON object with a `"type"` field identifying the message kind, or
//! - A bare non-JSON string, treated as a [`ControlMessage::UserMessage`].
//!
//! The [`parse_control_message`] function is a pure parser with no I/O — it takes
//! a single line string and returns the decoded message, if any.

use crate::runner::PermissionResponse;

/// A decoded stdin control message.
#[derive(Debug, PartialEq)]
pub enum ControlMessage {
    /// A new user prompt. Either parsed from `{"type":"user_message","text":"..."}` or
    /// from a bare non-JSON line for terminal ergonomics.
    UserMessage(String),
    /// Graceful session close (`{"type":"end_session"}`).
    EndSession,
    /// HITL gate response (`{"type":"hitl_response","text":"..."}`).
    HitlResponse(String),
    /// Tool permission decision.
    PermissionResponse {
        response: PermissionResponse,
        /// When `true`, the tool should be auto-approved for the remainder of the session.
        allow_for_session: bool,
    },
    /// Pause the current step.
    Pause,
    /// Resume a paused step.
    Resume,
    /// Kill the current step.
    Kill,
}

/// Parse a single line from the stdin control stream.
///
/// Returns `None` for empty lines or lines with an unrecognised JSON `type` that
/// should be silently ignored (e.g. future protocol extensions). The caller
/// decides what to do with each variant.
pub fn parse_control_message(line: &str) -> Option<ControlMessage> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try to parse as JSON. Non-JSON lines → UserMessage (terminal ergonomics).
    let json: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Some(ControlMessage::UserMessage(trimmed.to_string())),
    };

    match json.get("type").and_then(|t| t.as_str()) {
        Some("user_message") => {
            let text = json
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(ControlMessage::UserMessage(text))
        }
        Some("end_session") => Some(ControlMessage::EndSession),
        Some("hitl_response") => {
            let text = json
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(ControlMessage::HitlResponse(text))
        }
        Some("permission_response") => {
            let allowed = json
                .get("allowed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let allow_for_session = json
                .get("allow_for_session")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let response = if allowed {
                PermissionResponse::Allow
            } else {
                let reason = json
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                PermissionResponse::Deny(reason)
            };
            Some(ControlMessage::PermissionResponse {
                response,
                allow_for_session,
            })
        }
        Some("pause") => Some(ControlMessage::Pause),
        Some("resume") => Some(ControlMessage::Resume),
        Some("kill") => Some(ControlMessage::Kill),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_line_returns_none() {
        assert!(parse_control_message("").is_none());
        assert!(parse_control_message("   ").is_none());
    }

    #[test]
    fn bare_non_json_line_becomes_user_message() {
        let msg = parse_control_message("hello world");
        assert_eq!(
            msg,
            Some(ControlMessage::UserMessage("hello world".to_string()))
        );
    }

    #[test]
    fn user_message_json_is_parsed() {
        let msg = parse_control_message(r#"{"type":"user_message","text":"do something"}"#);
        assert_eq!(
            msg,
            Some(ControlMessage::UserMessage("do something".to_string()))
        );
    }

    #[test]
    fn end_session_is_parsed() {
        let msg = parse_control_message(r#"{"type":"end_session"}"#);
        assert_eq!(msg, Some(ControlMessage::EndSession));
    }

    #[test]
    fn hitl_response_is_parsed() {
        let msg = parse_control_message(r#"{"type":"hitl_response","text":"approved"}"#);
        assert_eq!(
            msg,
            Some(ControlMessage::HitlResponse("approved".to_string()))
        );
    }

    #[test]
    fn permission_response_allow_is_parsed() {
        let msg = parse_control_message(r#"{"type":"permission_response","allowed":true}"#);
        assert_eq!(
            msg,
            Some(ControlMessage::PermissionResponse {
                response: PermissionResponse::Allow,
                allow_for_session: false,
            })
        );
    }

    #[test]
    fn permission_response_allow_for_session_is_parsed() {
        let msg = parse_control_message(
            r#"{"type":"permission_response","allowed":true,"allow_for_session":true}"#,
        );
        assert_eq!(
            msg,
            Some(ControlMessage::PermissionResponse {
                response: PermissionResponse::Allow,
                allow_for_session: true,
            })
        );
    }

    #[test]
    fn permission_response_deny_carries_reason() {
        let msg = parse_control_message(
            r#"{"type":"permission_response","allowed":false,"reason":"not safe"}"#,
        );
        assert_eq!(
            msg,
            Some(ControlMessage::PermissionResponse {
                response: PermissionResponse::Deny("not safe".to_string()),
                allow_for_session: false,
            })
        );
    }

    #[test]
    fn pause_resume_kill_are_parsed() {
        assert_eq!(
            parse_control_message(r#"{"type":"pause"}"#),
            Some(ControlMessage::Pause)
        );
        assert_eq!(
            parse_control_message(r#"{"type":"resume"}"#),
            Some(ControlMessage::Resume)
        );
        assert_eq!(
            parse_control_message(r#"{"type":"kill"}"#),
            Some(ControlMessage::Kill)
        );
    }

    #[test]
    fn unknown_type_returns_none() {
        assert!(parse_control_message(r#"{"type":"future_extension","data":42}"#).is_none());
    }
}
