use ail_core::runner::{stub::StubRunner, InvokeOptions, Runner};

/// SPEC §8 — Runner trait is object-safe and returns RunResult
#[test]
fn stub_runner_satisfies_runner_trait() {
    let runner: Box<dyn Runner> = Box::new(StubRunner::new("test response"));
    let result = runner.invoke("any prompt", InvokeOptions::default()).unwrap();
    assert_eq!(result.response, "test response");
}

/// SPEC §8 — runner result carries cost and session_id
#[test]
fn stub_runner_result_has_cost_and_session_id() {
    let runner = StubRunner::new("response");
    let result = runner.invoke("prompt", InvokeOptions::default()).unwrap();
    assert!(result.cost_usd.is_some());
    assert!(result.session_id.is_some());
}

/// Integration test — requires claude CLI outside Claude Code session.
/// Run with: cargo nextest run -- --ignored
#[test]
#[ignore]
fn claude_cli_runner_returns_non_empty_response() {
    use ail_core::runner::claude::ClaudeCliRunner;
    let runner = ClaudeCliRunner::new();
    let result = runner
        .invoke("Reply with exactly the word: hello", InvokeOptions::default())
        .unwrap();
    assert!(!result.response.is_empty());
    assert!(result.cost_usd.is_some());
}
