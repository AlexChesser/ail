mod cli;

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

                let mut session = ail_core::session::Session::new(pipeline, prompt);
                let runner = ail_core::runner::claude::ClaudeCliRunner::new();

                match ail_core::executor::execute(&mut session, &runner) {
                    Ok(()) => {
                        if let Some(last) = session.turn_log.last_response() {
                            println!("{last}");
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
