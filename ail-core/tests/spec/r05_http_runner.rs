//! Specification r05 — HTTP Runner contract.
//!
//! Covers: no-model error, RunResult field invariants, and stub-server-backed
//! integration tests that exercise the full HttpRunner pipeline without needing
//! a live Ollama instance.
//!
//! Factory construction tests live in s08_multi_runner.rs alongside the other factory tests.
//! Live tests require a running Ollama instance and are marked #[ignore].

use ail_core::error::error_types;
use ail_core::runner::http::{HttpRunner, HttpRunnerConfig, HttpSessionStore};
use ail_core::runner::{InvokeOptions, Runner};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use stub_llm::{StubLlmServer, StubResponse};

fn fresh_store() -> HttpSessionStore {
    Arc::new(Mutex::new(HashMap::new()))
}

/// invoke() without a model on either config or options returns RUNNER_INVOCATION_FAILED
/// before contacting any server.
#[test]
fn invoke_no_model_returns_invocation_failed() {
    // Port 1 is reserved/refused on all platforms — ensures no accidental HTTP call.
    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            ..HttpRunnerConfig::default()
        },
        fresh_store(),
    );
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

// ── Inline HTTP stub ─────────────────────────────────────────────────────────

use std::io::{BufRead, BufReader, Read as _, Write};
use std::net::TcpListener;

/// Start a tiny HTTP/1.1 stub that returns canned responses.
/// Returns `(base_url, request_bodies)` where `request_bodies` is an `Arc<Mutex<Vec<String>>>`
/// that records the raw request bodies received by the stub.
fn start_stub(responses: Vec<&'static str>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let base_url = format!("http://{addr}");
    let bodies = Arc::new(Mutex::new(Vec::<String>::new()));
    let bodies_clone = Arc::clone(&bodies);

    std::thread::spawn(move || {
        for response_body in responses {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                // Read request line + headers
                let mut content_length: usize = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(val) = line.strip_prefix("Content-Length: ") {
                        content_length = val.trim().parse().unwrap_or(0);
                    } else if let Some(val) = line.strip_prefix("content-length: ") {
                        content_length = val.trim().parse().unwrap_or(0);
                    }
                }
                // Read body
                let mut body = vec![0u8; content_length];
                let _ = reader.read_exact(&mut body);
                bodies_clone
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).to_string());

                // Write response
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        }
    });

    (base_url, bodies)
}

/// Start a stub that accepts a connection but never responds (for cancel tests).
fn start_hanging_stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let base_url = format!("http://{addr}");

    std::thread::spawn(move || {
        // Accept the connection but never write a response.
        if let Ok((_stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
    });

    base_url
}

fn ok_response(content: &str) -> String {
    format!(
        r#"{{"model":"test-model","choices":[{{"message":{{"role":"assistant","content":"{content}"}}}}],"usage":{{"prompt_tokens":10,"completion_tokens":5}}}}"#,
    )
}

// ── Cancellation ─────────────────────────────────────────────────────────────

/// cancel_token aborts the HTTP request within the event-listener latency,
/// not after the full read timeout (300s).
#[test]
fn cancel_token_aborts_http_request_within_event_latency() {
    let base_url = start_hanging_stub();
    let store = fresh_store();
    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url,
            default_model: Some("test".to_string()),
            ..HttpRunnerConfig::default()
        },
        store,
    );

    let token = ail_core::runner::CancelToken::new();
    let token_clone = token.clone();

    // Cancel after 100ms
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        token_clone.cancel();
    });

    let start = std::time::Instant::now();
    let err = runner
        .invoke(
            "hello",
            InvokeOptions {
                model: Some("test".to_string()),
                cancel_token: Some(token),
                ..InvokeOptions::default()
            },
        )
        .unwrap_err();

    let elapsed = start.elapsed();
    assert_eq!(
        err.error_type(),
        error_types::RUNNER_CANCELLED,
        "expected RUNNER_CANCELLED, got: {} ({})",
        err.error_type(),
        err.detail()
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "should abort within 5s, took {:?}",
        elapsed
    );
}

// ── Shared session store ─────────────────────────────────────────────────────

/// Two HttpRunner instances sharing a store see each other's sessions.
#[test]
fn shared_store_preserves_session_across_instances() {
    let resp1 = ok_response("first response");
    let resp2 = ok_response("second response");
    let resp1_static: &'static str = Box::leak(resp1.into_boxed_str());
    let resp2_static: &'static str = Box::leak(resp2.into_boxed_str());

    let (base_url, bodies) = start_stub(vec![resp1_static, resp2_static]);

    let store = fresh_store();
    let runner1 = HttpRunner::new(
        HttpRunnerConfig {
            base_url: base_url.clone(),
            default_model: Some("test".to_string()),
            ..HttpRunnerConfig::default()
        },
        store.clone(),
    );
    let runner2 = HttpRunner::new(
        HttpRunnerConfig {
            base_url,
            default_model: Some("test".to_string()),
            ..HttpRunnerConfig::default()
        },
        store,
    );

    // First invoke on runner1 — creates session
    let result1 = runner1
        .invoke(
            "hello",
            InvokeOptions {
                model: Some("test".to_string()),
                system_prompt: Some("You are helpful.".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();
    let session_id = result1.session_id.unwrap();

    // Second invoke on runner2 — resumes the session from shared store
    let _result2 = runner2
        .invoke(
            "followup",
            InvokeOptions {
                model: Some("test".to_string()),
                resume_session_id: Some(session_id),
                ..InvokeOptions::default()
            },
        )
        .unwrap();

    // Verify the second request body contains the full history
    let recorded = bodies.lock().unwrap();
    assert_eq!(recorded.len(), 2);
    let body2: serde_json::Value = serde_json::from_str(&recorded[1]).unwrap();
    let messages = body2["messages"].as_array().unwrap();
    // Should have: system + user("hello") + assistant("first response") + user("followup")
    assert!(
        messages.len() >= 4,
        "expected >= 4 messages in second request, got {}",
        messages.len()
    );
}

// ── Resume-miss fallthrough ──────────────────────────────────────────────────

/// Resuming with an unknown session ID rebuilds the system prompt instead of
/// sending a context-free request.
#[test]
fn resume_unknown_session_id_rebuilds_system_prompt() {
    let resp = ok_response("ok");
    let resp_static: &'static str = Box::leak(resp.into_boxed_str());
    let (base_url, bodies) = start_stub(vec![resp_static]);

    let store = fresh_store();
    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url,
            default_model: Some("test".to_string()),
            ..HttpRunnerConfig::default()
        },
        store,
    );

    // Resume with a nonexistent session ID
    let _result = runner
        .invoke(
            "hello",
            InvokeOptions {
                model: Some("test".to_string()),
                resume_session_id: Some("nonexistent-uuid".to_string()),
                system_prompt: Some("Be helpful.".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();

    let recorded = bodies.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    let body: serde_json::Value = serde_json::from_str(&recorded[0]).unwrap();
    let messages = body["messages"].as_array().unwrap();
    // Should have system prompt + user message (not just user alone)
    assert!(
        messages.len() >= 2,
        "expected >= 2 messages (system + user), got {}",
        messages.len()
    );
    assert_eq!(
        messages[0]["role"], "system",
        "first message should be system prompt"
    );
}

// ── Sliding window ───────────────────────────────────────────────────────────

/// max_history_messages truncates old messages but keeps system prompt.
#[test]
fn sliding_window_truncates_history() {
    // We'll do 3 invocations with resume, then check the 3rd request body.
    // With max_history_messages=4, the 3rd request should have system + last 4 messages.
    let r1 = ok_response("r1");
    let r2 = ok_response("r2");
    let r3 = ok_response("r3");
    let r1_static: &'static str = Box::leak(r1.into_boxed_str());
    let r2_static: &'static str = Box::leak(r2.into_boxed_str());
    let r3_static: &'static str = Box::leak(r3.into_boxed_str());

    let (base_url, bodies) = start_stub(vec![r1_static, r2_static, r3_static]);
    let store = fresh_store();

    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url,
            default_model: Some("test".to_string()),
            max_history_messages: Some(4),
            ..HttpRunnerConfig::default()
        },
        store,
    );

    // Turn 1: fresh
    let result1 = runner
        .invoke(
            "msg1",
            InvokeOptions {
                model: Some("test".to_string()),
                system_prompt: Some("sys".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();
    let sid = result1.session_id.unwrap();

    // Turn 2: resume
    let _result2 = runner
        .invoke(
            "msg2",
            InvokeOptions {
                model: Some("test".to_string()),
                resume_session_id: Some(sid.clone()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();

    // Turn 3: resume again
    let _result3 = runner
        .invoke(
            "msg3",
            InvokeOptions {
                model: Some("test".to_string()),
                resume_session_id: Some(sid),
                ..InvokeOptions::default()
            },
        )
        .unwrap();

    // Check the 3rd request body
    let recorded = bodies.lock().unwrap();
    assert_eq!(recorded.len(), 3);
    let body3: serde_json::Value = serde_json::from_str(&recorded[2]).unwrap();
    let messages = body3["messages"].as_array().unwrap();

    // Without truncation: system + user(msg1) + asst(r1) + user(msg2) + asst(r2) + user(msg3) = 6
    // With max_history_messages=4: system + last 4 of [user(msg1),asst(r1),user(msg2),asst(r2),user(msg3)]
    //   = system + asst(r1) + user(msg2) + asst(r2) + user(msg3) = 5
    assert_eq!(
        messages.len(),
        5,
        "expected 5 messages (system + 4 most recent), got {}",
        messages.len()
    );
    assert_eq!(messages[0]["role"], "system");
    // Last message should be user "msg3"
    assert_eq!(messages[messages.len() - 1]["content"], "msg3");
}

// ── Live tests (require a running Ollama instance) ────────────────────────────

/// Live — session_id is always Some and parses as a valid UUID.
#[test]
#[ignore = "requires a running Ollama instance"]
fn live_invoke_session_id_is_present_and_is_uuid() {
    let runner = HttpRunner::ollama("qwen3:0.6b", fresh_store());
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
    let runner = HttpRunner::ollama("qwen3:0.6b", fresh_store());
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
    let runner = HttpRunner::ollama("qwen3:0.6b", fresh_store());
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

// ── Stub-server-backed integration tests ─────────────────────────────────────
//
// These tests use stub-llm's `StubLlmServer` to serve pre-recorded responses,
// exercising the full HttpRunner pipeline (request building, HTTP call, response
// parsing, session continuity) without requiring a live Ollama instance.

/// Helper: create an HttpRunner pointed at the stub server.
fn runner_for(server: &StubLlmServer, model: &str) -> HttpRunner {
    HttpRunner::new(
        HttpRunnerConfig {
            base_url: server.base_url(),
            default_model: Some(model.to_string()),
            ..HttpRunnerConfig::default()
        },
        fresh_store(),
    )
}

/// Basic roundtrip: invoke returns the canned response content.
#[test]
fn stub_invoke_returns_canned_response() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "Hello from stub!".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = runner_for(&server, "test-model");
    let result = runner.invoke("hi", InvokeOptions::default()).unwrap();
    assert_eq!(result.response, "Hello from stub!");
}

/// Verify the model field is passed in the request body.
#[test]
fn stub_invoke_passes_model_in_request() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = runner_for(&server, "my-special-model");
    runner.invoke("test", InvokeOptions::default()).unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(body["model"].as_str().unwrap(), "my-special-model");
}

/// Verify Authorization header is sent when auth_token is set.
#[test]
fn stub_invoke_passes_auth_header() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url: server.base_url(),
            auth_token: Some("secret-token-123".to_string()),
            default_model: Some("m".to_string()),
            ..HttpRunnerConfig::default()
        },
        fresh_store(),
    );
    runner.invoke("test", InvokeOptions::default()).unwrap();

    let requests = server.requests();
    let auth_header = requests[0]
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"));
    assert!(
        auth_header.is_some(),
        "Authorization header should be present"
    );
    assert_eq!(
        auth_header.unwrap().1,
        "Bearer secret-token-123",
        "Authorization header should contain the token"
    );
}

/// Verify no Authorization header is sent when auth_token is None.
#[test]
fn stub_invoke_no_auth_header_when_absent() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = runner_for(&server, "m");
    runner.invoke("test", InvokeOptions::default()).unwrap();

    let requests = server.requests();
    let auth_header = requests[0]
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"));
    assert!(
        auth_header.is_none(),
        "Authorization header should NOT be present when no token is set"
    );
}

/// Verify system prompt is prepended as the first message.
#[test]
fn stub_invoke_system_prompt_in_messages() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = runner_for(&server, "m");
    runner
        .invoke(
            "hello",
            InvokeOptions {
                system_prompt: Some("You are a helpful assistant.".to_string()),
                ..InvokeOptions::default()
            },
        )
        .unwrap();

    let requests = server.requests();
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    let messages = body["messages"].as_array().unwrap();

    assert!(messages.len() >= 2, "should have system + user messages");
    assert_eq!(messages[0]["role"].as_str().unwrap(), "system");
    assert_eq!(
        messages[0]["content"].as_str().unwrap(),
        "You are a helpful assistant."
    );
    assert_eq!(messages[1]["role"].as_str().unwrap(), "user");
    assert_eq!(messages[1]["content"].as_str().unwrap(), "hello");
}

/// Verify session continuity: second call with resume includes full history.
#[test]
fn stub_invoke_session_continuity() {
    let server = StubLlmServer::new(vec![
        StubResponse::Success {
            content: "I remember blue.".to_string(),
            model: None,
            usage: None,
        },
        StubResponse::Success {
            content: "Your colour is blue.".to_string(),
            model: None,
            usage: None,
        },
    ]);
    // Both calls must share the same store for session continuity
    let store = fresh_store();
    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url: server.base_url(),
            default_model: Some("m".to_string()),
            ..HttpRunnerConfig::default()
        },
        store,
    );

    // First invocation
    let first = runner
        .invoke("My colour is blue.", InvokeOptions::default())
        .unwrap();
    let session_id = first.session_id.clone().unwrap();

    // Second invocation with resume
    let _second = runner
        .invoke(
            "What is my colour?",
            InvokeOptions {
                resume_session_id: Some(session_id),
                ..InvokeOptions::default()
            },
        )
        .unwrap();

    // The second request should contain the full conversation history
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let body: serde_json::Value = serde_json::from_str(&requests[1].body).unwrap();
    let messages = body["messages"].as_array().unwrap();

    // Should have: user("My colour is blue"), assistant("I remember blue."), user("What is my colour?")
    assert!(
        messages.len() >= 3,
        "resumed session should have at least 3 messages, got {}",
        messages.len()
    );
    assert_eq!(messages[0]["role"].as_str().unwrap(), "user");
    assert_eq!(
        messages[0]["content"].as_str().unwrap(),
        "My colour is blue."
    );
    assert_eq!(messages[1]["role"].as_str().unwrap(), "assistant");
    assert_eq!(messages[1]["content"].as_str().unwrap(), "I remember blue.");
    assert_eq!(messages[2]["role"].as_str().unwrap(), "user");
    assert_eq!(
        messages[2]["content"].as_str().unwrap(),
        "What is my colour?"
    );
}

/// HTTP 500 from server returns RUNNER_INVOCATION_FAILED.
#[test]
fn stub_invoke_http_500_returns_error() {
    let server = StubLlmServer::new(vec![StubResponse::Raw {
        status_code: 500,
        body: "internal server error".to_string(),
    }]);
    let runner = runner_for(&server, "m");
    let err = runner.invoke("test", InvokeOptions::default()).unwrap_err();
    assert_eq!(err.error_type(), error_types::RUNNER_INVOCATION_FAILED);
    assert!(
        err.detail().contains("500"),
        "error detail should mention status code 500, got: {}",
        err.detail()
    );
}

/// Malformed JSON response returns RUNNER_INVOCATION_FAILED.
#[test]
fn stub_invoke_malformed_json_returns_error() {
    let server = StubLlmServer::new(vec![StubResponse::Raw {
        status_code: 200,
        body: "this is not json at all {{{{".to_string(),
    }]);
    let runner = runner_for(&server, "m");
    let err = runner.invoke("test", InvokeOptions::default()).unwrap_err();
    assert_eq!(err.error_type(), error_types::RUNNER_INVOCATION_FAILED);
    assert!(
        err.detail().contains("parse") || err.detail().contains("JSON"),
        "error detail should mention JSON parsing, got: {}",
        err.detail()
    );
}

/// Verify usage tokens are extracted from the response.
#[test]
fn stub_invoke_usage_tokens_extracted() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: Some((42, 17)),
    }]);
    let runner = runner_for(&server, "m");
    let result = runner.invoke("test", InvokeOptions::default()).unwrap();
    assert_eq!(result.input_tokens, 42, "input_tokens should be 42");
    assert_eq!(result.output_tokens, 17, "output_tokens should be 17");
}

/// HttpRunner always returns cost_usd as None (no pricing tables).
#[test]
fn stub_invoke_cost_is_none() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = runner_for(&server, "m");
    let result = runner.invoke("test", InvokeOptions::default()).unwrap();
    assert!(result.cost_usd.is_none(), "cost_usd should always be None");
}

/// HttpRunner always returns empty tool_events (no tool support).
#[test]
fn stub_invoke_tool_events_empty() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = runner_for(&server, "m");
    let result = runner.invoke("test", InvokeOptions::default()).unwrap();
    assert!(
        result.tool_events.is_empty(),
        "tool_events should always be empty"
    );
}

/// Verify the think field is sent in the request body when configured.
#[test]
fn stub_invoke_think_field_sent() {
    let server = StubLlmServer::new(vec![StubResponse::Success {
        content: "ok".to_string(),
        model: None,
        usage: None,
    }]);
    let runner = HttpRunner::new(
        HttpRunnerConfig {
            base_url: server.base_url(),
            default_model: Some("m".to_string()),
            think: Some(false),
            ..HttpRunnerConfig::default()
        },
        fresh_store(),
    );
    runner.invoke("test", InvokeOptions::default()).unwrap();

    let requests = server.requests();
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(
        body["think"].as_bool(),
        Some(false),
        "think field should be false in request body"
    );
}
