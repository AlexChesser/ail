use ail_core::executor::ExecuteOutcome;
use ail_core::runner::{InvokeOptions, Runner};

/// Run `--once` / positional prompt in dry-run mode.
///
/// Uses the `DryRunRunner` to execute the full pipeline resolution — template
/// variable substitution, condition evaluation, step ordering — without making
/// any LLM API calls. Shell context steps execute normally (they are local and free).
///
/// Output clearly labels each step with `[DRY RUN]` and shows the resolved prompt
/// or shell command that would be sent.
pub fn run_dry_run(session: &mut ail_core::session::Session, runner: &dyn Runner, prompt: &str) {
    println!("[DRY RUN] Pipeline: {}", pipeline_label(session));
    println!("[DRY RUN] Run ID: {}", session.run_id);
    println!("[DRY RUN] Invocation prompt: {}", truncate(prompt, 200));
    println!();

    let has_invocation_step = session.has_invocation_step();

    if !has_invocation_step {
        println!("[DRY RUN] Step 0: invocation (implicit — host-managed)");
        println!("  Prompt: {}", truncate(prompt, 200));
        println!("  Runner: {} (DRY RUN — no LLM call)", session.runner_name);
        println!();

        let options = InvokeOptions {
            model: session.cli_provider.model.clone(),
            extensions: runner.build_extensions(&session.cli_provider),
            ..InvokeOptions::default()
        };
        match ail_core::executor::run_invocation_step(session, runner, prompt, options) {
            Ok(_result) => {
                println!("  Result: [DRY RUN] No LLM call made");
                println!();
            }
            Err(e) => {
                eprintln!("[DRY RUN] Invocation step failed: {e}");
                std::process::exit(1);
            }
        }
    }

    let total_steps = session.pipeline.steps.len();
    println!(
        "[DRY RUN] Pipeline has {} declared step{}",
        total_steps,
        if total_steps == 1 { "" } else { "s" }
    );
    println!();

    // Show each step's details before execution
    for (i, step) in session.pipeline.steps.iter().enumerate() {
        let step_type = match &step.body {
            ail_core::config::domain::StepBody::Prompt(_) => "prompt",
            ail_core::config::domain::StepBody::Skill { .. } => "skill",
            ail_core::config::domain::StepBody::SubPipeline { .. } => "pipeline",
            ail_core::config::domain::StepBody::NamedPipeline { .. } => "named-pipeline",
            ail_core::config::domain::StepBody::Action(_) => "action",
            ail_core::config::domain::StepBody::Context(
                ail_core::config::domain::ContextSource::Shell(_),
            ) => "context:shell",
            ail_core::config::domain::StepBody::DoWhile { .. } => "do_while",
        };

        let condition_note = match &step.condition {
            Some(ail_core::config::domain::Condition::Never) => " [SKIPPED: condition: never]",
            _ => "",
        };

        let runner_note = step
            .runner
            .as_deref()
            .map(|r| format!(" (runner: {r})"))
            .unwrap_or_default();

        let model_note = step
            .model
            .as_deref()
            .map(|m| format!(" (model: {m})"))
            .unwrap_or_default();

        println!(
            "[DRY RUN] Step {}: {} [{}]{}{}{}",
            i + 1,
            step.id.as_str(),
            step_type,
            condition_note,
            runner_note,
            model_note,
        );

        match &step.body {
            ail_core::config::domain::StepBody::Prompt(template) => {
                println!("  Template: {}", truncate(template, 300));
                println!("  Runner: DRY RUN — no LLM call");
            }
            ail_core::config::domain::StepBody::Context(
                ail_core::config::domain::ContextSource::Shell(cmd),
            ) => {
                println!("  Command: {cmd}");
                println!("  (shell steps execute normally in dry-run mode)");
            }
            ail_core::config::domain::StepBody::SubPipeline { path, prompt: p } => {
                println!("  Pipeline path: {path}");
                if let Some(p) = p {
                    println!("  Prompt override: {}", truncate(p, 200));
                }
            }
            ail_core::config::domain::StepBody::Action(kind) => {
                println!("  Action: {kind:?}");
            }
            ail_core::config::domain::StepBody::Skill { ref name } => {
                println!("  Skill: {name}");
            }
            ail_core::config::domain::StepBody::NamedPipeline { name, prompt: p } => {
                println!("  Named pipeline: {name}");
                if let Some(p) = p {
                    println!("  Prompt override: {}", truncate(p, 200));
                }
            }
            ail_core::config::domain::StepBody::DoWhile {
                max_iterations,
                exit_when,
                steps,
            } => {
                println!(
                    "  do_while: max_iterations={max_iterations}, {} inner step{}",
                    steps.len(),
                    if steps.len() == 1 { "" } else { "s" }
                );
                println!(
                    "  exit_when: {:?} {} {:?}",
                    exit_when.lhs,
                    format_condition_op(&exit_when.op),
                    exit_when.rhs
                );
                for (j, inner) in steps.iter().enumerate() {
                    println!("    Inner step {}: {}", j + 1, inner.id.as_str());
                }
            }
        }

        if let Some(on_result) = &step.on_result {
            println!("  on_result: {} branch(es)", on_result.len());
        }

        println!();
    }

    // Execute with dry-run runner (templates get resolved, shell steps run, no LLM calls)
    println!("[DRY RUN] --- Executing pipeline ---");
    println!();

    match ail_core::executor::execute(session, runner) {
        Ok(outcome) => {
            session.turn_log.record_run_finished("completed (dry run)");

            // Print resolved prompts and results from the turn log
            for entry in session.turn_log.entries() {
                if entry.step_id == "invocation" {
                    continue; // Already printed above
                }

                println!("[DRY RUN] Executed: {}", entry.step_id);
                println!("  Resolved prompt: {}", truncate(&entry.prompt, 300));

                if let Some(ref response) = entry.response {
                    println!("  Response: {}", truncate(response, 200));
                }
                if let Some(ref stdout) = entry.stdout {
                    if !stdout.is_empty() {
                        println!("  stdout: {}", truncate(stdout.trim(), 200));
                    }
                }
                if let Some(ref stderr) = entry.stderr {
                    if !stderr.is_empty() {
                        println!("  stderr: {}", truncate(stderr.trim(), 200));
                    }
                }
                if let Some(exit_code) = entry.exit_code {
                    println!("  exit_code: {exit_code}");
                }
                println!();
            }

            match outcome {
                ExecuteOutcome::Completed => {
                    println!("[DRY RUN] Pipeline completed successfully.");
                }
                ExecuteOutcome::Break { step_id } => {
                    println!("[DRY RUN] Pipeline stopped early via break at step '{step_id}'.");
                }
                ExecuteOutcome::Error(msg) => {
                    println!("[DRY RUN] Pipeline completed with error: {msg}");
                }
            }

            println!("[DRY RUN] No LLM API calls were made.");
        }
        Err(e) => {
            session.turn_log.record_run_finished("failed (dry run)");
            eprintln!("[DRY RUN] Pipeline execution failed: {e}");
            std::process::exit(1);
        }
    }
}

fn pipeline_label(session: &ail_core::session::Session) -> String {
    session
        .pipeline
        .source
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(passthrough)".to_string())
}

fn format_condition_op(op: &ail_core::config::domain::ConditionOp) -> &'static str {
    match op {
        ail_core::config::domain::ConditionOp::Eq => "==",
        ail_core::config::domain::ConditionOp::Ne => "!=",
        ail_core::config::domain::ConditionOp::Contains => "contains",
        ail_core::config::domain::ConditionOp::StartsWith => "starts_with",
        ail_core::config::domain::ConditionOp::EndsWith => "ends_with",
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    let single_line = s.replace('\n', "\\n");
    if single_line.len() > max_len {
        format!("{}...", &single_line[..max_len])
    } else {
        single_line
    }
}
