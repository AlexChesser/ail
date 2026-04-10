//! Pure formatting functions for ail-log/1 output format (spec/runner/r04-ail-log-format.md).

#![allow(clippy::result_large_err)]

use crate::logs::StepRow;

/// Format a sequence of steps as ail-log/1 markdown.
///
/// Returns a string starting with `ail-log/1` and containing complete turn blocks
/// with thinking, response, cost lines, and error callouts as appropriate.
pub fn format_run_as_ail_log(steps: &[StepRow]) -> String {
    let mut output = String::from("ail-log/1\n");

    if steps.is_empty() {
        return output;
    }

    // Build a map of step_id → index (1-based for turn numbering).
    // We count unique step_ids that have step_completed or step_started events
    // to determine turn numbers.
    let mut step_indices: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut turn_counter = 0;

    for step in steps {
        if !step_indices.contains_key(&step.step_id) {
            turn_counter += 1;
            step_indices.insert(step.step_id.clone(), turn_counter);
        }
    }

    // Group steps by step_id to handle multiple events per step.
    let mut step_groups: std::collections::HashMap<String, Vec<&StepRow>> =
        std::collections::HashMap::new();
    for step in steps {
        step_groups
            .entry(step.step_id.clone())
            .or_default()
            .push(step);
    }

    // Iterate through unique step_ids in order of first appearance.
    let mut seen = std::collections::HashSet::new();
    for step in steps {
        if seen.insert(&step.step_id) {
            let turn_number = step_indices[&step.step_id];
            let group = &step_groups[&step.step_id];

            format_step(&mut output, turn_number, &step.step_id, group);
        }
    }

    output
}

/// Format a single step (which may have multiple rows: step_started, thinking, response, etc).
fn format_step(output: &mut String, turn_number: usize, step_id: &str, rows: &[&StepRow]) {
    // Write turn header
    output.push_str(&format!("\n## Turn {turn_number} — `{step_id}`\n"));

    // Collect data from all rows for this step.
    let mut thinking = String::new();
    let mut response = String::new();
    let mut cost_usd: Option<f64> = None;
    let mut input_tokens: Option<i64> = None;
    let mut output_tokens: Option<i64> = None;
    let mut stdout: Option<String> = None;
    let mut stderr: Option<String> = None;
    let mut failed = false;
    let mut failure_detail = String::new();
    // tool_events: collect from the step_completed row (has the richest data).
    // Use the first row with non-empty tool_events.
    let mut tool_events: &[crate::runner::ToolEvent] = &[];

    for row in rows {
        if let Some(t) = &row.thinking {
            if !t.is_empty() {
                thinking = t.clone();
            }
        }
        if let Some(r) = &row.response {
            if !r.is_empty() {
                response = r.clone();
            }
        }
        if row.cost_usd.is_some() {
            cost_usd = row.cost_usd;
        }
        if row.input_tokens.is_some() {
            input_tokens = row.input_tokens;
        }
        if row.output_tokens.is_some() {
            output_tokens = row.output_tokens;
        }
        if row.stdout.is_some() {
            stdout = row.stdout.clone();
        }
        if row.stderr.is_some() {
            stderr = row.stderr.clone();
        }
        if row.event_type == "step_failed" {
            failed = true;
            failure_detail = row.response.as_deref().unwrap_or("Step failed").to_string();
        }
        if !row.tool_events.is_empty() {
            tool_events = &row.tool_events;
        }
    }

    // 1. Emit thinking block if present
    if !thinking.is_empty() {
        output.push_str("\n:::thinking\n");
        output.push_str(&thinking);
        output.push_str("\n:::\n");
    }

    // 2. Emit tool call/result pairs in seq order
    for te in tool_events {
        if te.event_type == "tool_call" {
            output.push_str(&format!(
                "\n:::tool-call name=\"{}\"\n{}\n:::\n",
                te.tool_name, te.content_json
            ));
        } else if te.event_type == "tool_result" {
            output.push_str(&format!(
                "\n:::tool-result name=\"{}\"\n{}\n:::\n",
                te.tool_name, te.content_json
            ));
        }
    }

    // 3. Emit response text if present
    if !response.is_empty() && !failed {
        output.push('\n');
        output.push_str(&response);
        output.push('\n');
    }

    // 4. Emit stdio blocks for context steps
    if let Some(ref out) = stdout {
        if !out.is_empty() {
            output.push_str("\n:::stdio stream=\"stdout\"\n");
            output.push_str(out);
            output.push_str("\n:::\n");
        }
    }
    if let Some(ref err) = stderr {
        if !err.is_empty() {
            output.push_str("\n:::stdio stream=\"stderr\"\n");
            output.push_str(err);
            output.push_str("\n:::\n");
        }
    }

    // 5. Emit cost line or error callout
    if failed {
        output.push_str("\n> [!WARNING]\n");
        output.push_str(&format!("> **Step failed:** {failure_detail}\n"));
    } else if cost_usd.is_some() || input_tokens.is_some() || output_tokens.is_some() {
        output.push_str("\n---\n");
        output.push_str(&format_cost_line(cost_usd, input_tokens, output_tokens));
        output.push('\n');
    }
}

/// Format a cost line per spec/runner/r04.
pub(crate) fn format_cost_line(
    cost_usd: Option<f64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> String {
    match (cost_usd, input_tokens, output_tokens) {
        (Some(cost), Some(in_tok), Some(out_tok)) => {
            format!("_Cost: ${cost:.4} | {in_tok}in / {out_tok}out tokens_")
        }
        (Some(cost), _, _) => {
            format!("_Cost: ${cost:.4}_")
        }
        _ => "_No cost (context step)_".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::StepRow;

    // -------------------------------------------------------------------------
    // format_cost_line — private helper tested via pub(crate) visibility
    // -------------------------------------------------------------------------

    #[test]
    fn cost_line_all_three_fields_present() {
        let line = format_cost_line(Some(0.0025), Some(200), Some(80));
        assert_eq!(line, "_Cost: $0.0025 | 200in / 80out tokens_");
    }

    #[test]
    fn cost_line_cost_only_no_tokens() {
        let line = format_cost_line(Some(1.5), None, None);
        assert_eq!(line, "_Cost: $1.5000_");
    }

    #[test]
    fn cost_line_cost_with_input_tokens_only() {
        // Only cost + input — output is None → falls to (Some(cost), _, _) branch.
        let line = format_cost_line(Some(0.001), Some(10), None);
        assert_eq!(line, "_Cost: $0.0010_");
    }

    #[test]
    fn cost_line_no_cost_at_all_returns_no_cost_label() {
        let line = format_cost_line(None, None, None);
        assert_eq!(line, "_No cost (context step)_");
    }

    #[test]
    fn cost_line_no_cost_with_tokens_still_returns_no_cost_label() {
        // Tokens without a cost amount still fall to the wildcard branch.
        let line = format_cost_line(None, Some(100), Some(50));
        assert_eq!(line, "_No cost (context step)_");
    }

    #[test]
    fn cost_line_uses_four_decimal_places() {
        // 1/3 rounds to 4 decimal places.
        let line = format_cost_line(Some(1.0 / 3.0), Some(1), Some(1));
        assert!(
            line.contains("$0.3333"),
            "expected 4 decimal places, got: {line}"
        );
    }

    // -------------------------------------------------------------------------
    // format_run_as_ail_log — top-level function
    // -------------------------------------------------------------------------

    fn make_row(step_id: &str, event_type: &str, response: Option<&str>) -> StepRow {
        StepRow {
            step_id: step_id.to_string(),
            event_type: event_type.to_string(),
            prompt: None,
            response: response.map(|s| s.to_string()),
            thinking: None,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            stdout: None,
            stderr: None,
            exit_code: None,
            recorded_at: 0,
            tool_events: vec![],
        }
    }

    #[test]
    fn format_run_starts_with_version_header() {
        let output = format_run_as_ail_log(&[make_row("s", "step_completed", Some("hi"))]);
        assert!(output.starts_with("ail-log/1\n"));
    }

    #[test]
    fn format_run_empty_input_is_header_only() {
        let output = format_run_as_ail_log(&[]);
        assert_eq!(output, "ail-log/1\n");
    }

    #[test]
    fn format_run_stdout_block_is_emitted() {
        let mut row = make_row("ctx", "step_completed", None);
        row.stdout = Some("hello stdout".to_string());
        let output = format_run_as_ail_log(&[row]);
        assert!(
            output.contains(":::stdio stream=\"stdout\""),
            "stdout block missing"
        );
        assert!(output.contains("hello stdout"), "stdout content missing");
    }

    #[test]
    fn format_run_stderr_block_is_emitted() {
        let mut row = make_row("ctx", "step_completed", None);
        row.stderr = Some("error output".to_string());
        let output = format_run_as_ail_log(&[row]);
        assert!(
            output.contains(":::stdio stream=\"stderr\""),
            "stderr block missing"
        );
        assert!(output.contains("error output"), "stderr content missing");
    }

    #[test]
    fn format_run_empty_stdout_is_not_emitted() {
        let mut row = make_row("ctx", "step_completed", None);
        row.stdout = Some(String::new());
        let output = format_run_as_ail_log(&[row]);
        assert!(
            !output.contains(":::stdio stream=\"stdout\""),
            "empty stdout should not produce a block"
        );
    }

    #[test]
    fn format_run_step_failed_emits_warning_callout() {
        let row = make_row("deploy", "step_failed", Some("boom"));
        let output = format_run_as_ail_log(&[row]);
        assert!(output.contains("> [!WARNING]"), "warning callout missing");
        assert!(output.contains("boom"), "failure detail missing");
    }

    #[test]
    fn format_run_cost_separator_precedes_cost_line() {
        let mut row = make_row("s", "step_completed", Some("response"));
        row.cost_usd = Some(0.001);
        row.input_tokens = Some(10);
        row.output_tokens = Some(5);
        let output = format_run_as_ail_log(&[row]);
        let sep_pos = output.find("\n---\n").expect("--- separator missing");
        let cost_pos = output.find("_Cost:").expect("cost line missing");
        assert!(cost_pos > sep_pos, "cost line must come after separator");
    }

    #[test]
    fn format_run_response_not_emitted_for_failed_step() {
        let row = make_row("s", "step_failed", Some("failure detail"));
        let output = format_run_as_ail_log(&[row]);
        // The failure_detail is rendered inside the callout — but NOT as a plain response paragraph.
        // The plain response block is skipped when `failed` is true.
        assert!(
            !output.contains("\nfailure detail\n"),
            "failed step response should not appear as plain text"
        );
    }
}
