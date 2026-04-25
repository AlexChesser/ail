//! Tests for spec/core/s24-log-command.md and spec/runner/r04-ail-log-format.md
//!
//! Coverage: ail-log/1 formatting, turn numbering, cost lines, error callouts,
//! thinking blocks, and version header requirements.

use ail_core::formatter::format_run_as_ail_log;
use ail_core::logs::StepRow;

#[test]
fn test_version_header_is_first_line() {
    let steps = vec![StepRow {
        step_id: "invocation".to_string(),
        event_type: "step_completed".to_string(),
        prompt: Some("Hello".to_string()),
        response: Some("Hi there!".to_string()),
        thinking: None,
        cost_usd: Some(0.001),
        input_tokens: Some(10),
        output_tokens: Some(5),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    let lines: Vec<&str> = output.lines().collect();
    assert!(!lines.is_empty(), "output must have at least one line");
    assert_eq!(lines[0], "ail-log/1", "first line must be version header");
}

#[test]
fn test_turn_header_format() {
    let steps = vec![StepRow {
        step_id: "my_step".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some("test response".to_string()),
        thinking: None,
        cost_usd: Some(0.001),
        input_tokens: Some(10),
        output_tokens: Some(5),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    assert!(
        output.contains("## Turn 1 — `my_step`"),
        "turn header must match spec format"
    );
}

#[test]
fn test_multiple_turns_numbered_sequentially() {
    let steps = vec![
        StepRow {
            step_id: "step_1".to_string(),
            event_type: "step_completed".to_string(),
            prompt: None,
            response: Some("Response 1".to_string()),
            thinking: None,
            cost_usd: Some(0.001),
            input_tokens: Some(10),
            output_tokens: Some(5),
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 1000,
            tool_events: vec![],
        },
        StepRow {
            step_id: "step_2".to_string(),
            event_type: "step_completed".to_string(),
            prompt: None,
            response: Some("Response 2".to_string()),
            thinking: None,
            cost_usd: Some(0.002),
            input_tokens: Some(20),
            output_tokens: Some(10),
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 2000,
            tool_events: vec![],
        },
    ];

    let output = format_run_as_ail_log(&steps);
    assert!(output.contains("## Turn 1 — `step_1`"));
    assert!(output.contains("## Turn 2 — `step_2`"));
}

#[test]
fn test_thinking_block_format() {
    let steps = vec![StepRow {
        step_id: "invocation".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some("Response text".to_string()),
        thinking: Some("Let me think about this...".to_string()),
        cost_usd: Some(0.001),
        input_tokens: Some(10),
        output_tokens: Some(5),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    assert!(output.contains(":::thinking"));
    assert!(output.contains("Let me think about this..."));
    assert!(output.contains(":::"));
}

#[test]
fn test_response_text_is_plain() {
    let response_text = "Here's some **bold** text and a [link](http://example.com).";
    let steps = vec![StepRow {
        step_id: "review".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some(response_text.to_string()),
        thinking: None,
        cost_usd: Some(0.001),
        input_tokens: Some(10),
        output_tokens: Some(5),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    assert!(
        output.contains(response_text),
        "response should be plain text, no HTML"
    );
    assert!(!output.contains("<html>"), "no HTML tags allowed");
    assert!(!output.contains("<div>"), "no HTML tags allowed");
}

#[test]
fn test_cost_line_format_with_tokens() {
    let steps = vec![StepRow {
        step_id: "invocation".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some("Response".to_string()),
        thinking: None,
        cost_usd: Some(0.0015),
        input_tokens: Some(120),
        output_tokens: Some(45),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    assert!(
        output.contains("_Cost: $0.0015 | 120in / 45out tokens_"),
        "cost line must follow r04 format"
    );
}

#[test]
fn test_cost_line_formatting_four_decimals() {
    let steps = vec![StepRow {
        step_id: "step1".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some("Result".to_string()),
        thinking: None,
        cost_usd: Some(1.23456), // will be formatted to 4 decimal places
        input_tokens: Some(100),
        output_tokens: Some(50),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    // Should format to 4 decimal places
    assert!(output.contains("$1.2346"), "cost must use 4 decimal places");
}

#[test]
fn test_error_callout_format() {
    let steps = vec![StepRow {
        step_id: "deploy".to_string(),
        event_type: "step_failed".to_string(),
        prompt: None,
        response: Some("Deployment key not found in environment".to_string()),
        thinking: None,
        cost_usd: None,
        input_tokens: None,
        output_tokens: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    assert!(
        output.contains("> [!WARNING]"),
        "error must use warning callout"
    );
    assert!(
        output.contains("> **Step failed:**"),
        "error must have step label"
    );
    assert!(
        output.contains("Deployment key not found"),
        "error detail must be present"
    );
}

#[test]
fn test_no_html_in_any_format() {
    let steps = vec![StepRow {
        step_id: "step1".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some("Response with <tag>".to_string()),
        thinking: Some("Thinking <content>".to_string()),
        cost_usd: Some(0.001),
        input_tokens: Some(10),
        output_tokens: Some(5),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    // Check that any HTML-like tags are not transformed; they're kept as literals
    // if present in the data. The formatter doesn't inject HTML.
    assert!(
        !output.contains("<div>"),
        "formatter must not inject div tags"
    );
    assert!(
        !output.contains("<span>"),
        "formatter must not inject span tags"
    );
    assert!(
        !output.contains("<html>"),
        "formatter must not inject html tags"
    );
}

#[test]
fn test_empty_steps_list() {
    let steps: Vec<StepRow> = vec![];
    let output = format_run_as_ail_log(&steps);
    assert_eq!(
        output, "ail-log/1\n",
        "empty steps should only output version header"
    );
}

#[test]
fn test_multiple_rows_per_step_aggregated() {
    // Simulate step_started + step_completed for the same step_id.
    let steps = vec![
        StepRow {
            step_id: "my_step".to_string(),
            event_type: "step_started".to_string(),
            prompt: None,
            response: None,
            thinking: None,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 1000,
            tool_events: vec![],
        },
        StepRow {
            step_id: "my_step".to_string(),
            event_type: "step_completed".to_string(),
            prompt: None,
            response: Some("Final response".to_string()),
            thinking: Some("Final thinking".to_string()),
            cost_usd: Some(0.001),
            input_tokens: Some(10),
            output_tokens: Some(5),
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 2000,
            tool_events: vec![],
        },
    ];

    let output = format_run_as_ail_log(&steps);
    assert!(
        output.contains("## Turn 1 — `my_step`"),
        "should appear once"
    );
    assert!(
        output.lines().filter(|l| l.contains("## Turn 1")).count() == 1,
        "turn header should appear exactly once for repeated step_id"
    );
    assert!(output.contains("Final response"));
    assert!(output.contains("Final thinking"));
}

#[test]
fn test_step_without_response() {
    let steps = vec![StepRow {
        step_id: "validate".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: None,
        thinking: None,
        cost_usd: Some(0.0001),
        input_tokens: Some(5),
        output_tokens: Some(2),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    assert!(output.contains("## Turn 1 — `validate`"));
    assert!(output.contains("_Cost: $0.0001 | 5in / 2out tokens_"));
}

#[test]
fn test_cost_line_placement_after_response() {
    let steps = vec![StepRow {
        step_id: "step1".to_string(),
        event_type: "step_completed".to_string(),
        prompt: None,
        response: Some("This is the response.".to_string()),
        thinking: None,
        cost_usd: Some(0.001),
        input_tokens: Some(10),
        output_tokens: Some(5),
        stdout: None,
        stderr: None,
        exit_code: None,
        recorded_at: 1000,
        tool_events: vec![],
    }];

    let output = format_run_as_ail_log(&steps);
    let response_pos = output.find("This is the response.").unwrap();
    let cost_pos = output.find("_Cost:").unwrap();
    assert!(
        cost_pos > response_pos,
        "cost line must come after response"
    );
}

#[test]
fn test_real_world_pipeline_output() {
    let steps = vec![
        StepRow {
            step_id: "invocation".to_string(),
            event_type: "step_completed".to_string(),
            prompt: Some("Add a fizzbuzz function".to_string()),
            response: Some("I'll help you add a fizzbuzz function. Let me first understand the context.".to_string()),
            thinking: Some("The user is asking for a fizzbuzz implementation.".to_string()),
            cost_usd: Some(0.0008),
            input_tokens: Some(85),
            output_tokens: Some(42),
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 1000,
            tool_events: vec![],
        },
        StepRow {
            step_id: "code_review".to_string(),
            event_type: "step_completed".to_string(),
            prompt: None,
            response: Some("Here's the fizzbuzz implementation:\n\n```go\nfunc FizzBuzz(n int) string {\n    // implementation\n}\n```".to_string()),
            thinking: Some("Now I should provide a working implementation.".to_string()),
            cost_usd: Some(0.0012),
            input_tokens: Some(156),
            output_tokens: Some(87),
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 2000,
            tool_events: vec![],
        },
    ];

    let output = format_run_as_ail_log(&steps);

    // Check structure.
    assert!(output.starts_with("ail-log/1\n"));
    assert!(output.contains("## Turn 1 — `invocation`"));
    assert!(output.contains("## Turn 2 — `code_review`"));

    // Check thinking blocks.
    assert!(output.contains(":::thinking"));

    // Check costs.
    assert!(output.contains("_Cost: $0.0008 | 85in / 42out tokens_"));
    assert!(output.contains("_Cost: $0.0012 | 156in / 87out tokens_"));

    // Check code block preservation.
    assert!(output.contains("```go"));
    assert!(output.contains("func FizzBuzz"));
}
