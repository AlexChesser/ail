use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ail",
    version = ail_core::version(),
    about = "Artificial Intelligence Loops — the control plane for how agents behave after the human stops typing."
)]
pub struct Cli {
    /// Run a single non-interactive prompt and exit.
    #[arg(long, value_name = "PROMPT")]
    pub once: Option<String>,

    /// Path to the pipeline YAML file. Overrides automatic discovery.
    #[arg(long, value_name = "PATH")]
    pub pipeline: Option<PathBuf>,

    /// Disable TUI and emit structured JSON to stdout.
    #[arg(long)]
    pub headless: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Resolve and output the full pipeline. Alias: mp
    #[command(name = "materialize", visible_alias = "mp")]
    Materialize {
        /// Path to the pipeline YAML file. Overrides automatic discovery.
        #[arg(long, value_name = "PATH")]
        pipeline: Option<PathBuf>,

        /// Write output to a file instead of stdout.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Validate a pipeline file without running it.
    Validate {
        /// Path to the pipeline YAML file to validate. Overrides automatic discovery.
        #[arg(long, value_name = "PATH")]
        pipeline: Option<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hello world"]).unwrap();
        assert_eq!(cli.once.as_deref(), Some("hello world"));
    }

    #[test]
    fn pipeline_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--pipeline", "/tmp/test.ail.yaml"]).unwrap();
        assert_eq!(cli.pipeline, Some(PathBuf::from("/tmp/test.ail.yaml")));
    }

    #[test]
    fn headless_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--headless"]).unwrap();
        assert!(cli.headless);
    }

    #[test]
    fn materialize_subcommand_parses() {
        let cli = Cli::try_parse_from(["ail", "materialize"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Materialize { .. })));
    }

    #[test]
    fn materialize_alias_mp_parses() {
        let cli = Cli::try_parse_from(["ail", "mp"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Materialize { .. })));
    }

    #[test]
    fn materialize_with_out_parses() {
        let cli =
            Cli::try_parse_from(["ail", "materialize", "--out", "/tmp/out.yaml"]).unwrap();
        if let Some(Commands::Materialize { out, .. }) = cli.command {
            assert_eq!(out, Some(PathBuf::from("/tmp/out.yaml")));
        } else {
            panic!("expected Materialize command");
        }
    }

    #[test]
    fn validate_subcommand_parses() {
        let cli = Cli::try_parse_from(["ail", "validate"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Validate { .. })));
    }

    #[test]
    fn validate_with_pipeline_flag_parses() {
        let cli =
            Cli::try_parse_from(["ail", "validate", "--pipeline", "/tmp/test.ail.yaml"]).unwrap();
        if let Some(Commands::Validate { pipeline }) = cli.command {
            assert_eq!(pipeline, Some(PathBuf::from("/tmp/test.ail.yaml")));
        } else {
            panic!("expected Validate command");
        }
    }

    #[test]
    fn no_args_produces_empty_cli() {
        let cli = Cli::try_parse_from(["ail"]).unwrap();
        assert!(cli.once.is_none());
        assert!(cli.pipeline.is_none());
        assert!(!cli.headless);
        assert!(cli.command.is_none());
    }
}
