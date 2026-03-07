mod cli;

use ail_core::runner::Runner;
use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    tracing_subscriber::fmt().json().init();

    let cli = Cli::parse();

    tracing::info!(event = "startup", version = ail_core::version());

    match cli.command {
        Some(Commands::MaterializeChain { pipeline, out }) => {
            let path = match ail_core::config::discovery::discover(pipeline) {
                Some(p) => p,
                None => {
                    eprintln!("No pipeline file found.");
                    std::process::exit(1);
                }
            };
            match ail_core::config::load(&path) {
                Ok(p) => {
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
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
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

                // Invocation 1: run the user's --once prompt through Claude.
                match runner.invoke(&prompt, None) {
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

                // Invocation 2+: execute pipeline steps, resuming the session.
                match ail_core::executor::execute(&mut session, &runner) {
                    Ok(()) => {
                        if let Some(last) = session.turn_log.last_response() {
                            println!("\n--- dont_be_stupid ---\n{last}");
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
