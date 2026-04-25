mod ask_user_hook;
mod ask_user_types;
mod check_permission_hook;
mod cli;
mod command;
mod control_bridge;
mod delete;
mod discover;
mod dry_run;
mod help;
mod log;
mod logs;
mod materialize;
mod once_json;
mod once_text;
mod spec;
mod stdio;
mod validate;

use ail_core::runner::factory::RunnerFactory;
use clap::Parser;
use cli::{Cli, Commands, OutputFormat};
use command::CommandOutcome;

/// Discover and load a pipeline from an optional explicit path, falling back to
/// automatic discovery and then passthrough mode. Exits with code 1 on load error.
fn load_pipeline(explicit_path: Option<std::path::PathBuf>) -> ail_core::config::domain::Pipeline {
    match discover::resolve_or_passthrough(explicit_path) {
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

/// Initialise tracing conditionally based on output format.
fn init_tracing(output_format: &OutputFormat) {
    match output_format {
        OutputFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_writer(std::io::stderr)
                .init();
            tracing::info!(event = "startup", version = ail_core::version());
        }
        OutputFormat::Text => {
            // Silent by default in text mode — developers can use RUST_LOG to enable tracing.
            let _ = tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .try_init();
        }
    }
}

fn exit_with(outcome: CommandOutcome) -> ! {
    std::process::exit(outcome.exit_code())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    init_tracing(&cli.output_format);

    // Effective prompt: positional wins, --once as long-form alias.
    let effective_prompt = cli.prompt.clone().or(cli.once.clone());

    match (effective_prompt, cli.command) {
        (Some(prompt), None) => {
            tracing::info!(
                event = "once",
                headless = cli.headless,
                dry_run = cli.dry_run
            );

            let pipeline = load_pipeline(cli.pipeline);

            let mut session = ail_core::session::Session::new(pipeline, prompt.clone());
            session.headless = cli.headless;
            session.cli_provider = ail_core::config::domain::ProviderConfig {
                model: cli.model.clone(),
                base_url: cli.provider_url.clone(),
                auth_token: cli.provider_token.clone(),
                ..Default::default()
            };

            if cli.dry_run {
                let runner = ail_core::runner::dry_run::DryRunRunner::new();
                dry_run::run_dry_run(&mut session, &runner, &prompt);
            } else {
                let runner = match RunnerFactory::build_default(
                    cli.headless,
                    &session.http_session_store,
                    &session.pipeline.defaults,
                ) {
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
        }
        (None, Some(cmd)) => match cmd {
            Commands::Spec {
                format,
                section,
                list,
                core,
                runner,
            } => {
                let fmt = match spec::SpecFormat::parse(&format) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                };
                let cmd = spec::SpecCommand::new(fmt, section, list, core, runner);
                exit_with(cmd.execute());
            }
            Commands::Delete {
                run_id,
                force,
                json,
            } => {
                let cmd = delete::DeleteCommand::new(run_id, force, json);
                exit_with(cmd.execute());
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
            Commands::Materialize {
                pipeline,
                out,
                expand_pipelines,
            } => {
                let p = load_pipeline(pipeline);
                let cmd = materialize::MaterializeCommand::new(p, out, expand_pipelines);
                exit_with(cmd.execute());
            }
            Commands::Validate {
                pipeline,
                output_format,
            } => {
                let cmd = validate::ValidateCommand::new(pipeline, output_format);
                exit_with(cmd.execute());
            }
            Commands::Init {
                template,
                force,
                dry_run,
            } => {
                let args = ail_init::InitArgs {
                    template,
                    force,
                    dry_run,
                };
                if let Err(e) = ail_init::run(args) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            Commands::Stdio {
                message,
                stream,
                pipeline,
                model,
                provider_url,
                provider_token,
            } => {
                tracing::info!(
                    event = "stdio",
                    one_shot = message.is_some(),
                    stream = stream
                );
                let discovered_pipeline = load_pipeline(cli.pipeline.or(pipeline));
                let cli_provider = ail_core::config::domain::ProviderConfig {
                    model,
                    base_url: provider_url,
                    auth_token: provider_token,
                    ..Default::default()
                };
                // Shared HTTP session store for the stdio session.
                let http_store: ail_core::runner::http::HttpSessionStore =
                    std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
                let runner = match RunnerFactory::build_default(
                    true,
                    &http_store,
                    &discovered_pipeline.defaults,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                };
                let result = if stream {
                    stdio::run_chat_stream(
                        discovered_pipeline,
                        cli_provider,
                        runner.as_ref(),
                        message,
                    )
                    .await
                } else {
                    stdio::run_chat_text(
                        discovered_pipeline,
                        cli_provider,
                        runner.as_ref(),
                        message,
                    )
                };
                if let Err(e) = result {
                    eprintln!("stdio error: {e}");
                    std::process::exit(1);
                }
            }
        },
        (None, None) => {
            help::print_landing_page();
            std::process::exit(0);
        }
        (Some(_), Some(_)) => {
            // clap prevents this via conflicts_with, but handle defensively.
            unreachable!("clap should have rejected prompt + subcommand combination");
        }
    }
}
