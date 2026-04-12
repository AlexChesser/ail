//! Specification r05 — HTTP Runner contract.
//!
//! Covers: no-model error, RunResult field invariants.
//! Factory construction tests live in s08_multi_runner.rs alongside the other factory tests.
//! Live tests require a running Ollama instance and are marked #[ignore].

use ail_core::error::error_types;
use ail_core::runner::http::{HttpRunner, HttpRunnerConfig};
use ail_core::runner::{InvokeOptions, Runner};

/// invoke() without a model on either config or options returns RUNNER_INVOCATION_FAILED
/// before contacting any server.
#[test]
fn invoke_no_model_returns_invocation_failed() {
    // Port 1 is reserved/refused on all platforms — ensures no accidental HTTP call.
    let runner = HttpRunner::new(HttpRunnerConfig {
        base_url: "http://127.0.0.1:1".to_string(),
        auth_token: None,
        default_model: None,
        think: None,
    });
    let err = runner
        .invoke("hello", InvokeOptions::default())
        .unwrap_err();
    assert_eq!(
        err.error_type(),
        error_types::RUNNER_INVOCATION_FAILED,
        "expected RUNNER_INVOCATION_FAILED, got: {}",
        err.error_type()
    );
    assert!(
        err.detail().contains("no model specified"),
        "detail should mention missing model, got: {}",
        err.detail()
    );
}

// ── Live tests (require a running Ollama instance) ────────────────────────────

/// Live — session_id is always Some and parses as a valid UUID.
#[test]
#[ignore = "requires a running Ollama instance"]
fn live_invoke_session_id_is_present_and_is_uuid() {
    let runner = HttpRunner::ollama("qwen3:0.6b");
    let result = runner
        .invoke(
            "Say one word: OK",
            InvokeOptions {
                model: Some("qwen3:0.6b".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();
    let id = result.session_id.expect("session_id must always be Some");
    uuid::Uuid::parse_str(&id).expect("session_id must be a valid UUID");
}

/// Live — cost_usd is always None (HTTP runner has no pricing tables).
#[test]
#[ignore = "requires a running Ollama instance"]
fn live_invoke_cost_usd_is_always_none() {
    let runner = HttpRunner::ollama("qwen3:0.6b");
    let result = runner
        .invoke(
            "Say one word: OK",
            InvokeOptions {
                model: Some("qwen3:0.6b".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();
    assert!(result.cost_usd.is_none(), "cost_usd must always be None");
}

/// Live — tool_events is always empty (HTTP runner does not support tool calls).
#[test]
#[ignore = "requires a running Ollama instance"]
fn live_invoke_tool_events_is_always_empty() {
    let runner = HttpRunner::ollama("qwen3:0.6b");
    let result = runner
        .invoke(
            "Say one word: OK",
            InvokeOptions {
                model: Some("qwen3:0.6b".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();
    assert!(
        result.tool_events.is_empty(),
        "tool_events must always be empty"
    );
}
