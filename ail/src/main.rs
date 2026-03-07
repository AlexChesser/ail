mod cli;

use ail_core::runner::{InvokeOptions, Runner};
use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    tracing_subscriber::fmt().json().init();

    let cli = Cli::parse();

    tracing::info!(event = "startup", version = ail_core::version());

    match cli.command {
        Some(Commands::MaterializeChain { pipeline, out }) => {
            let pipeline_path = ail_core::config::discovery::discover(pipeline);
            let p = match pipeline_path {
                Some(ref path) => match ail_core::config::load(path) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                },
                None => ail_core::config::domain::Pipeline::passthrough(),
            };
            let output = ail_core::materialize::materialize(&p);
            match out {
                Some(out_path) => {
                    if let Err(e) = std::fs::write(&out_path, &output) {
                        eprintln!("Failed to write to {}: {e}", out_path.display());
                        std::process::exit(1);
                    }
                }
                None => print!("{output}"),
            }
        }
        Some(Commands::Validate { pipeline }) => {
            let path = match ail_core::config::discovery::discover(pipeline) {
                Some(p) => p,
                None => {
                    eprintln!("No pipeline file found.");
                    std::process::exit(1);
                }
            };
            match ail_core::config::load(&path) {
                Ok(p) => {
                    println!("Pipeline valid: {} step(s)", p.steps.len());
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        None => {
            if let Some(prompt) = cli.once {
                tracing::info!(event = "once", headless = cli.headless);

                let pipeline_path = ail_core::config::discovery::discover(cli.pipeline);
                let pipeline = match pipeline_path {
                    Some(ref path) => match ail_core::config::load(path) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    },
                    None => ail_core::config::domain::Pipeline::passthrough(),
                };

                let mut session = ail_core::session::Session::new(pipeline, prompt.clone());
                let runner = ail_core::runner::claude::ClaudeCliRunner::new();

                // If the pipeline does not declare an invocation step, the host runs it
                // with default settings before handing off to the executor (SPEC §4.1).
                let has_invocation_step = session
                    .pipeline
                    .steps
                    .first()
                    .map(|s| s.id.as_str() == "invocation")
                    .unwrap_or(false);

                if !has_invocation_step {
                    match runner.invoke(&prompt, InvokeOptions::default()) {
                        Ok(result) => {
                            println!("{}", result.response);
                            session.turn_log.append(ail_core::session::TurnEntry {
                                step_id: "invocation".to_string(),
                                prompt: prompt.clone(),
                                response: Some(result.response),
                                timestamp: std::time::SystemTime::now(),
                                cost_usd: result.cost_usd,
                                runner_session_id: result.session_id,
                            });
                        }
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    }
                }

                // Execute pipeline steps. If the pipeline declares an invocation step,
                // the executor runs it (with whatever config the user supplied). Subsequent
                // steps resume the session via last_runner_session_id (SPEC §4.1, §4.2).
                match ail_core::executor::execute(&mut session, &runner) {
                    Ok(()) => {
                        if has_invocation_step {
                            // Executor ran invocation — print its response now.
                            if let Some(resp) =
                                session.turn_log.response_for_step("invocation")
                            {
                                println!("{resp}");
                            }
                        }
                        // Print the last non-invocation step response, if any.
                        if let Some(entry) = session
                            .turn_log
                            .entries()
                            .iter()
                            .rev()
                            .find(|e| e.step_id != "invocation" && e.response.is_some())
                        {
                            println!(
                                "\n--- {} ---\n{}",
                                entry.step_id,
                                entry.response.as_deref().unwrap_or("")
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
            } else {
                tracing::info!(event = "repl_stub");
                eprintln!("ail: interactive REPL not yet implemented in v0.0.1");
            }
        }
    }
}
