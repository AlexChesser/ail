mod ask_user_hook;
mod ask_user_types;
mod chat;
mod check_permission_hook;
mod cli;
mod control_bridge;
mod delete;
mod log;
mod logs;
mod materialize;
mod once_json;
mod once_text;
mod validate;

use ail_core::runner::factory::RunnerFactory;
use clap::Parser;
use cli::{Cli, Commands, OutputFormat};

/// Discover and load a pipeline from an optional explicit path, falling back to
/// automatic discovery and then passthrough mode. Exits with code 1 on load error.
fn load_pipeline(explicit_path: Option<std::path::PathBuf>) -> ail_core::config::domain::Pipeline {
    match ail_core::config::discovery::discover(explicit_path) {
        Some(ref path) => match ail_core::config::load(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        None => ail_core::config::domain::Pipeline::passthrough(),
    }
}

/// Initialise tracing. Always writes structured JSON logs to stderr.
fn init_tracing() {
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    init_tracing();

    tracing::info!(event = "startup", version = ail_core::version());

    // Effective prompt: positional wins, --once as long-form alias.
    let effective_prompt = cli.prompt.clone().or(cli.once.clone());

    match (effective_prompt, cli.command) {
        (Some(prompt), None) => {
            tracing::info!(event = "once", headless = cli.headless);

            let pipeline = load_pipeline(cli.pipeline);

            let mut session = ail_core::session::Session::new(pipeline, prompt.clone());
            session.cli_provider = ail_core::config::domain::ProviderConfig {
                model: cli.model.clone(),
                base_url: cli.provider_url.clone(),
                auth_token: cli.provider_token.clone(),
                input_cost_per_1k: None,
                output_cost_per_1k: None,
            };
            let runner = match RunnerFactory::build_default(cli.headless) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            match cli.output_format {
                OutputFormat::Text => once_text::run_once_text(
                    &mut session,
                    runner.as_ref(),
                    &prompt,
                    cli.show_thinking,
                    cli.watch,
                    cli.show_work,
                ),
                OutputFormat::Json => {
                    once_json::run_once_json(&mut session, runner.as_ref(), &prompt).await
                }
            }
        }
        (None, Some(cmd)) => match cmd {
            Commands::Delete {
                run_id,
                force,
                json,
            } => {
                if let Err(e) = delete::handle_delete(run_id, force, json) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            Commands::Logs {
                session,
                query,
                format,
                tail,
                limit,
            } => {
                logs::run_logs_command(session, query, format, tail, limit);
            }
            Commands::Log {
                run_id,
                format,
                follow,
            } => {
                log::run_log_command(run_id, &format, follow);
            }
            Commands::AskUserHook { socket } => {
                ask_user_hook::run(&socket);
            }
            Commands::CheckPermissionHook { socket } => {
                check_permission_hook::run(&socket);
            }
            Commands::Materialize { pipeline, out } => {
                let p = load_pipeline(pipeline);
                materialize::handle_materialize(p, out);
            }
            Commands::Validate {
                pipeline,
                output_format,
            } => {
                validate::handle_validate(pipeline, output_format);
            }
            Commands::Chat {
                message,
                stream,
                pipeline,
                model,
                provider_url,
                provider_token,
            } => {
                tracing::info!(
                    event = "chat",
                    one_shot = message.is_some(),
                    stream = stream
                );
                let discovered_pipeline = load_pipeline(cli.pipeline.or(pipeline));
                let cli_provider = ail_core::config::domain::ProviderConfig {
                    model,
                    base_url: provider_url,
                    auth_token: provider_token,
                    input_cost_per_1k: None,
                    output_cost_per_1k: None,
                };
                let runner = match RunnerFactory::build_default(true) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                };
                let result = if stream {
                    chat::run_chat_stream(
                        discovered_pipeline,
                        cli_provider,
                        runner.as_ref(),
                        message,
                    )
                    .await
                } else {
                    chat::run_chat_text(discovered_pipeline, cli_provider, runner.as_ref(), message)
                };
                if let Err(e) = result {
                    eprintln!("chat error: {e}");
                    std::process::exit(1);
                }
            }
        },
        (None, None) => {
            // No prompt and no subcommand — print usage hint and exit.
            eprintln!("Usage: ail <PROMPT> [OPTIONS]");
            eprintln!("       ail --once <PROMPT> [OPTIONS]");
            eprintln!("       ail <SUBCOMMAND> [OPTIONS]");
            eprintln!();
            eprintln!("Run `ail --help` for full usage.");
            std::process::exit(0);
        }
        (Some(_), Some(_)) => {
            // clap prevents this via conflicts_with, but handle defensively.
            unreachable!("clap should have rejected prompt + subcommand combination");
        }
    }
}
