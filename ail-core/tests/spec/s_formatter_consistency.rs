//! Tests for cross-format consistency between Rust and TypeScript formatters.
//!
//! Issue #39 (D1): Ensures the single source of truth (Rust binary) produces
//! stable, reproducible output that external consumers (including the VS Code
//! extension) can rely on for rendering and parsing.
//!
//! Coverage: Load a fixture JSONL file, parse it into StepRow structs,
//! call format_run_as_ail_log(), and assert byte-equality with a golden file.

use ail_core::formatter::format_run_as_ail_log;
use ail_core::logs::StepRow;
use std::fs;
use std::path::Path;

/// Load and parse the consistency fixture JSONL file.
///
/// The fixture contains one of every event type that the formatter must handle:
/// - run_started (header metadata, not in StepRow)
/// - step_started for "my_step"
/// - step_completed with thinking, response, costs
/// - step_completed without thinking (optional response)
/// - step_failed (failed step)
/// - Another step_started and step_completed
fn load_consistency_fixture() -> Vec<StepRow> {
    // Relative to workspace root (where Cargo.toml is)
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-fixtures/consistency-fixture.jsonl");

    let content =
        fs::read_to_string(&fixture_path).expect("Failed to read consistency-fixture.jsonl");

    let mut steps = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let json: serde_json::Value =
            serde_json::from_str(line).expect("Failed to parse JSONL line as JSON");

        // Skip run_started events; they don't map to StepRow.
        let event_type = json["event_type"]
            .as_str()
            .or_else(|| json["type"].as_str())
            .expect("Missing event_type or type field");

        if event_type == "run_started" {
            continue;
        }

        // Parse the event into a StepRow.
        let step_id = json["step_id"]
            .as_str()
            .expect("Missing step_id")
            .to_string();

        let response = json["response"].as_str().map(|s| s.to_string());
        let thinking = json["thinking"].as_str().map(|s| s.to_string());
        let cost_usd = json["cost_usd"].as_f64();
        let input_tokens = json["input_tokens"].as_i64();
        let output_tokens = json["output_tokens"].as_i64();
        let prompt = json["prompt"].as_str().map(|s| s.to_string());
        let recorded_at = json["recorded_at"].as_i64().expect("Missing recorded_at");

        let step_row = StepRow {
            step_id,
            event_type: event_type.to_string(),
            prompt,
            response,
            thinking,
            cost_usd,
            input_tokens,
            output_tokens,
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at,
            tool_events: vec![],
        };

        steps.push(step_row);
    }

    steps
}

/// Load the golden output file.
fn load_golden_file() -> String {
    // Relative to workspace root (where Cargo.toml is)
    let golden_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test-fixtures/consistency-fixture.ail-log");
    fs::read_to_string(&golden_path).expect("Failed to read consistency-fixture.ail-log")
}

#[test]
fn test_consistency_fixture_matches_golden_file() {
    let steps = load_consistency_fixture();
    let output = format_run_as_ail_log(&steps);
    let golden = load_golden_file();

    if output != golden {
        // Show a diff-like output for debugging.
        let output_lines: Vec<&str> = output.lines().collect();
        let golden_lines: Vec<&str> = golden.lines().collect();

        eprintln!("\n=== FORMATTER OUTPUT ===");
        for (i, line) in output_lines.iter().enumerate() {
            eprintln!("{:3}: {}", i + 1, line);
        }

        eprintln!("\n=== GOLDEN FILE ===");
        for (i, line) in golden_lines.iter().enumerate() {
            eprintln!("{:3}: {}", i + 1, line);
        }

        eprintln!("\n=== DIFF ===");
        let max_lines = output_lines.len().max(golden_lines.len());
        for i in 0..max_lines {
            let output_line = output_lines.get(i).copied().unwrap_or("");
            let golden_line = golden_lines.get(i).copied().unwrap_or("");
            if output_line != golden_line {
                eprintln!("Line {}: DIFFER", i + 1);
                eprintln!("  Output: {:?}", output_line);
                eprintln!("  Golden: {:?}", golden_line);
            }
        }
    }

    assert_eq!(
        output, golden,
        "Formatted output must byte-equal the golden file. See eprintln output above for diff."
    );
}

#[test]
fn test_consistency_fixture_structure() {
    let steps = load_consistency_fixture();

    // Verify we have steps for all three turns.
    let step_ids: Vec<&str> = steps.iter().map(|s| s.step_id.as_str()).collect();

    assert!(step_ids.contains(&"my_step"));
    assert!(step_ids.contains(&"my_other_step"));
    assert!(step_ids.contains(&"security_check"));

    // Verify we have both step_started and step_completed events for my_step.
    let my_step_events: Vec<&str> = steps
        .iter()
        .filter(|s| s.step_id == "my_step")
        .map(|s| s.event_type.as_str())
        .collect();

    assert!(
        my_step_events.contains(&"step_started"),
        "my_step should have step_started event"
    );
    assert!(
        my_step_events.contains(&"step_completed"),
        "my_step should have step_completed event"
    );

    // Verify security_check has a step_failed event.
    let security_check_events: Vec<&str> = steps
        .iter()
        .filter(|s| s.step_id == "security_check")
        .map(|s| s.event_type.as_str())
        .collect();

    assert!(
        security_check_events.contains(&"step_failed"),
        "security_check should have step_failed event"
    );
}

#[test]
fn test_consistency_fixture_has_thinking() {
    let steps = load_consistency_fixture();

    // my_step should have thinking content.
    let my_step = steps
        .iter()
        .find(|s| s.step_id == "my_step" && s.thinking.is_some())
        .expect("my_step should have thinking");

    assert!(
        my_step
            .thinking
            .as_ref()
            .unwrap()
            .contains("off-by-one error"),
        "thinking should contain domain-specific content"
    );
}

#[test]
fn test_consistency_fixture_has_costs() {
    let steps = load_consistency_fixture();

    // my_step and my_other_step should have costs.
    let my_step = steps
        .iter()
        .find(|s| s.step_id == "my_step" && s.cost_usd.is_some())
        .expect("my_step should have cost");

    assert_eq!(my_step.cost_usd, Some(0.0042));
    assert_eq!(my_step.input_tokens, Some(150));
    assert_eq!(my_step.output_tokens, Some(85));
}

#[test]
fn test_consistency_fixture_has_error() {
    let steps = load_consistency_fixture();

    // security_check should have a step_failed event with error detail.
    let security_check = steps
        .iter()
        .find(|s| s.step_id == "security_check" && s.event_type == "step_failed")
        .expect("security_check should have step_failed");

    assert!(
        security_check.response.is_some(),
        "failed step should have error detail"
    );
    assert!(
        security_check
            .response
            .as_ref()
            .unwrap()
            .contains("memory leak"),
        "error detail should be meaningful"
    );
}
