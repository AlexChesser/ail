//! SPEC §11 — template variable resolution.

use ail_core::config::domain::Pipeline;
use ail_core::error::error_types;
use ail_core::runner::ToolEvent;
use ail_core::session::{NullProvider, Session, TurnEntry};
use ail_core::template::resolve;
use std::time::SystemTime;

fn make_session() -> Session {
    Session::new(Pipeline::passthrough(), "original prompt".to_string())
        .with_log_provider(Box::new(NullProvider))
}

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
        thinking: None,
        tool_events: vec![],
        modified: None,
    });
}

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
        thinking: None,
        tool_events: vec![],
        modified: None,
    });
}

// ── 1. Literal passthrough ────────────────────────────────────────────────

#[test]
fn no_variables_unchanged() {
    let session = make_session();
    let result = resolve("no variables here", &session).unwrap();
    assert_eq!(result, "no variables here");
}

#[test]
fn empty_string_passes_through() {
    let session = make_session();
    let result = resolve("", &session).unwrap();
    assert_eq!(result, "");
}

#[test]
fn whitespace_only_passes_through() {
    let session = make_session();
    let result = resolve("   \t\n  ", &session).unwrap();
    assert_eq!(result, "   \t\n  ");
}

// ── 2. step.invocation.prompt ─────────────────────────────────────────────

#[test]
fn invocation_prompt_resolves() {
    let session = make_session();
    let result = resolve("{{ step.invocation.prompt }}", &session).unwrap();
    assert_eq!(result, "original prompt");
}

#[test]
fn invocation_prompt_embedded_in_text() {
    let session = make_session();
    let result = resolve("prefix {{ step.invocation.prompt }} suffix", &session).unwrap();
    assert_eq!(result, "prefix original prompt suffix");
}

// ── 3. step.invocation.response ──────────────────────────────────────────

#[test]
fn invocation_response_resolves_after_entry() {
    let mut session = make_session();
    append_response(&mut session, "invocation", "the invocation reply");
    let result = resolve("{{ step.invocation.response }}", &session).unwrap();
    assert_eq!(result, "the invocation reply");
}

#[test]
fn invocation_response_missing_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.invocation.response }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 4. last_response ─────────────────────────────────────────────────────

#[test]
fn last_response_resolves_to_most_recent() {
    let mut session = make_session();
    append_response(&mut session, "step_a", "first");
    append_response(&mut session, "step_b", "second");
    let result = resolve("{{ last_response }}", &session).unwrap();
    assert_eq!(result, "second");
}

#[test]
fn last_response_no_entries_returns_error() {
    let session = make_session();
    let err = resolve("{{ last_response }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 5. env.<VAR> ─────────────────────────────────────────────────────────

#[test]
fn env_var_resolves() {
    // Safety: test-only mutation of environment; unique key to avoid collision.
    unsafe { std::env::set_var("AIL_TEMPLATE_TEST_VAR_12345", "hello_env") };
    let session = make_session();
    let result = resolve("{{ env.AIL_TEMPLATE_TEST_VAR_12345 }}", &session).unwrap();
    assert_eq!(result, "hello_env");
    unsafe { std::env::remove_var("AIL_TEMPLATE_TEST_VAR_12345") };
}

#[test]
fn env_missing_var_returns_error() {
    unsafe { std::env::remove_var("AIL_MISSING_VAR_XYZ_999") };
    let session = make_session();
    let err = resolve("{{ env.AIL_MISSING_VAR_XYZ_999 }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
    assert!(err.detail().contains("AIL_MISSING_VAR_XYZ_999"));
}

// ── 6. session.cwd ───────────────────────────────────────────────────────

#[test]
fn session_cwd_resolves_to_nonempty_string() {
    let session = make_session();
    let result = resolve("{{ session.cwd }}", &session).unwrap();
    // cwd is captured at Session::new time — must match session.cwd field.
    assert_eq!(result, session.cwd);
    assert!(!result.is_empty());
}

// ── 7. session.tool ──────────────────────────────────────────────────────

#[test]
fn session_tool_resolves_to_runner_name() {
    // session.runner_name defaults to "claude" when AIL_DEFAULT_RUNNER is unset.
    let mut session = make_session();
    // Explicitly set runner_name to verify the template reads the field, not a hardcoded string.
    session.runner_name = "ollama".to_string();
    let result = resolve("{{ session.tool }}", &session).unwrap();
    assert_eq!(result, "ollama");
}

#[test]
fn session_tool_default_is_claude_when_env_unset() {
    let session = make_session();
    // Default: AIL_DEFAULT_RUNNER not set → "claude"
    let result = resolve("{{ session.tool }}", &session).unwrap();
    // runner_name is set from env at Session::new() time; in tests it's "claude" (default).
    assert_eq!(result, session.runner_name);
}

// ── 8. pipeline.run_id ───────────────────────────────────────────────────

#[test]
fn pipeline_run_id_resolves() {
    let session = make_session();
    let result = resolve("{{ pipeline.run_id }}", &session).unwrap();
    assert_eq!(result, session.run_id);
}

#[test]
fn pipeline_run_id_is_nonempty_uuid_like() {
    let session = make_session();
    let result = resolve("{{ pipeline.run_id }}", &session).unwrap();
    // UUID v4 has exactly 4 hyphens
    assert_eq!(result.chars().filter(|&c| c == '-').count(), 4);
}

// ── 9. step.<id>.response ────────────────────────────────────────────────

#[test]
fn named_step_response_resolves() {
    let mut session = make_session();
    append_response(&mut session, "review", "looks good");
    let result = resolve("{{ step.review.response }}", &session).unwrap();
    assert_eq!(result, "looks good");
}

#[test]
fn named_step_response_missing_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.unknown_step.response }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
    assert!(err.detail().contains("unknown_step"));
}

#[test]
fn named_step_response_no_response_field_returns_error() {
    // Step exists but has no response (context step with only stdout)
    let mut session = make_session();
    append_context(&mut session, "ctx_step", "output", "", 0);
    let err = resolve("{{ step.ctx_step.response }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 10. step.<id>.stdout / stderr / exit_code / result ───────────────────

#[test]
fn step_stdout_resolves() {
    let mut session = make_session();
    append_context(&mut session, "shell1", "hello stdout", "some err", 0);
    let result = resolve("{{ step.shell1.stdout }}", &session).unwrap();
    assert_eq!(result, "hello stdout");
}

#[test]
fn step_stderr_resolves() {
    let mut session = make_session();
    append_context(&mut session, "shell1", "out", "warning: something", 0);
    let result = resolve("{{ step.shell1.stderr }}", &session).unwrap();
    assert_eq!(result, "warning: something");
}

#[test]
fn step_exit_code_resolves_as_string() {
    let mut session = make_session();
    append_context(&mut session, "shell1", "", "", 42);
    let result = resolve("{{ step.shell1.exit_code }}", &session).unwrap();
    assert_eq!(result, "42");
}

#[test]
fn step_exit_code_zero_resolves() {
    let mut session = make_session();
    append_context(&mut session, "shell1", "output", "", 0);
    let result = resolve("{{ step.shell1.exit_code }}", &session).unwrap();
    assert_eq!(result, "0");
}

#[test]
fn step_result_stdout_only() {
    let mut session = make_session();
    // stderr empty → result == stdout
    append_context(&mut session, "sh", "only out", "", 0);
    let result = resolve("{{ step.sh.result }}", &session).unwrap();
    assert_eq!(result, "only out");
}

#[test]
fn step_result_stderr_only() {
    let mut session = make_session();
    // stdout empty → result == stderr
    append_context(&mut session, "sh", "", "only err", 0);
    let result = resolve("{{ step.sh.result }}", &session).unwrap();
    assert_eq!(result, "only err");
}

#[test]
fn step_result_both_stdout_and_stderr() {
    let mut session = make_session();
    append_context(&mut session, "sh", "out", "err", 0);
    let result = resolve("{{ step.sh.result }}", &session).unwrap();
    assert_eq!(result, "out\nerr");
}

#[test]
fn step_stdout_missing_step_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.missing.stdout }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

#[test]
fn step_stderr_missing_step_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.missing.stderr }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

#[test]
fn step_exit_code_missing_step_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.missing.exit_code }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

#[test]
fn step_result_missing_step_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.missing.result }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 11. session.invocation_prompt alias ───────────────────────────────────

#[test]
fn session_invocation_prompt_alias_resolves() {
    let session = make_session();
    let result = resolve("{{ session.invocation_prompt }}", &session).unwrap();
    assert_eq!(result, "original prompt");
}

#[test]
fn session_invocation_prompt_alias_matches_canonical() {
    let session = make_session();
    let canonical = resolve("{{ step.invocation.prompt }}", &session).unwrap();
    let alias = resolve("{{ session.invocation_prompt }}", &session).unwrap();
    assert_eq!(canonical, alias);
}

// ── 12. Unknown variables return errors ───────────────────────────────────

#[test]
fn unknown_top_level_namespace_returns_error() {
    let session = make_session();
    let err = resolve("{{ totally.unknown.variable }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

#[test]
fn unknown_variable_error_contains_variable_name() {
    let session = make_session();
    let err = resolve("{{ totally.unknown.variable }}", &session).unwrap_err();
    assert!(err.detail().contains("totally.unknown.variable"));
}

#[test]
fn bare_word_returns_error() {
    let session = make_session();
    let err = resolve("{{ notavariable }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 13. Multiple variables in one template ────────────────────────────────

#[test]
fn multiple_variables_all_resolve() {
    let mut session = make_session();
    append_response(&mut session, "step_a", "answer_a");
    let tpl =
        "prompt={{ step.invocation.prompt }} step={{ step.step_a.response }} tool={{ session.tool }}";
    let result = resolve(tpl, &session).unwrap();
    assert_eq!(result, "prompt=original prompt step=answer_a tool=claude");
}

#[test]
fn two_same_variables_both_resolve() {
    let session = make_session();
    let result = resolve("{{ session.tool }}-{{ session.tool }}", &session).unwrap();
    assert_eq!(result, "claude-claude");
}

#[test]
fn partial_failure_returns_error_for_first_bad_variable() {
    let session = make_session();
    // First var is fine; second doesn't exist → should error
    let err = resolve(
        "{{ session.tool }} {{ step.nonexistent.response }}",
        &session,
    )
    .unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 14. Malformed / edge-case syntax ──────────────────────────────────────

#[test]
fn unclosed_brace_returns_error() {
    let session = make_session();
    let err = resolve("{{ unclosed", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
    assert!(err.detail().contains("}}"));
}

#[test]
fn unclosed_brace_with_text_after_returns_error() {
    let session = make_session();
    let err = resolve("prefix {{ no_close more text", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

#[test]
fn extra_braces_in_literal_text_not_treated_as_variable() {
    // Single braces are not template syntax
    let session = make_session();
    let result = resolve("{ not a variable }", &session).unwrap();
    assert_eq!(result, "{ not a variable }");
}

#[test]
fn variable_with_extra_whitespace_resolves() {
    // The variable trimming in the implementation strips leading/trailing whitespace
    let session = make_session();
    let result = resolve("{{   session.tool   }}", &session).unwrap();
    assert_eq!(result, "claude");
}

#[test]
fn empty_variable_returns_error() {
    let session = make_session();
    // {{}} → variable="" → unrecognised
    let err = resolve("{{}}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
}

// ── 15. Text surrounding variables is preserved ───────────────────────────

#[test]
fn text_before_variable_preserved() {
    let session = make_session();
    let result = resolve("hello {{ session.tool }}", &session).unwrap();
    assert_eq!(result, "hello claude");
}

#[test]
fn text_after_variable_preserved() {
    let session = make_session();
    let result = resolve("{{ session.tool }} world", &session).unwrap();
    assert_eq!(result, "claude world");
}

#[test]
fn text_before_and_after_variable_preserved() {
    let session = make_session();
    let result = resolve("A {{ session.tool }} B", &session).unwrap();
    assert_eq!(result, "A claude B");
}

// ── 16. step.result falls back to response for prompt steps ───────────────

#[test]
fn step_result_falls_back_to_response_for_prompt_step() {
    let mut session = make_session();
    // A prompt step has response but no stdout/stderr
    append_response(&mut session, "ask", "the answer");
    let result = resolve("{{ step.ask.result }}", &session).unwrap();
    assert_eq!(result, "the answer");
}

// ── 17. step.<id>.tool_calls ─────────────────────────────────────────────

#[test]
fn step_tool_calls_empty_array_when_no_events() {
    let mut session = make_session();
    append_response(&mut session, "ask", "response with no tools");
    let result = resolve("{{ step.ask.tool_calls }}", &session).unwrap();
    // Empty tool_events → serialises to "[]"
    assert_eq!(result, "[]");
}

#[test]
fn step_tool_calls_serialises_events_as_json() {
    let mut session = make_session();
    let event = ToolEvent {
        event_type: "tool_call".to_string(),
        tool_name: "Bash".to_string(),
        tool_id: "tool-abc".to_string(),
        content_json: "{\"command\":\"ls\"}".to_string(),
        seq: 0,
    };
    session.turn_log.append(TurnEntry {
        step_id: "run_check".to_string(),
        prompt: "do it".to_string(),
        response: Some("done".to_string()),
        timestamp: SystemTime::now(),
        cost_usd: None,
        input_tokens: 0,
        output_tokens: 0,
        runner_session_id: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        thinking: None,
        tool_events: vec![event],
        modified: None,
    });

    let result = resolve("{{ step.run_check.tool_calls }}", &session).unwrap();
    // Must be a valid JSON array containing the event fields
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("must be valid JSON");
    let arr = parsed.as_array().expect("must be array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["tool_name"], "Bash");
    assert_eq!(arr[0]["event_type"], "tool_call");
}

// ── 13. {{ step.<id>.modified }} (SPEC §13.2) ──────────────────────────

#[test]
fn step_modified_resolves_to_modified_output() {
    let mut session = make_session();
    let entry = TurnEntry::from_modify(
        "review_gate",
        "Please review".to_string(),
        "edited text".to_string(),
    );
    session.turn_log.append(entry);

    let result = resolve("{{ step.review_gate.modified }}", &session).unwrap();
    assert_eq!(result, "edited text");
}

#[test]
fn step_modified_missing_returns_error() {
    let session = make_session();
    let result = resolve("{{ step.no_gate.modified }}", &session);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type(),
        ail_core::error::error_types::TEMPLATE_UNRESOLVED
    );
}

#[test]
fn step_modified_none_when_not_a_modify_step_returns_error() {
    let mut session = make_session();
    // Append a regular prompt entry (modified is None).
    append_response(&mut session, "regular", "some response");
    let result = resolve("{{ step.regular.modified }}", &session);
    assert!(result.is_err());
}

#[test]
fn step_tool_calls_missing_step_returns_error() {
    let session = make_session();
    let err = resolve("{{ step.missing.tool_calls }}", &session).unwrap_err();
    assert_eq!(err.error_type(), error_types::TEMPLATE_UNRESOLVED);
    assert!(err.detail().contains("missing"));
}
