mod cli;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    tracing_subscriber::fmt().json().init();

    let cli = Cli::parse();

    tracing::info!(event = "startup", version = ail_core::version());

    match cli.command {
        Some(Commands::MaterializeChain { pipeline, out }) => {
            tracing::info!(event = "materialize_chain", ?pipeline, ?out);
            eprintln!("materialize-chain: not yet implemented");
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
                eprintln!("--once: not yet implemented (prompt: {prompt:?})");
            } else {
                tracing::info!(event = "repl_stub");
                eprintln!("ail: interactive REPL not yet implemented in v0.0.1");
            }
        }
    }
}
