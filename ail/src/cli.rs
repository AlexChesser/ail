use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Output format for pipeline execution results.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text output (default).
    #[default]
    Text,
    /// NDJSON event stream to stdout — one JSON object per line.
    Json,
}

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

    /// Override the model for all runner invocations (e.g. `gemma3:1b` for Ollama).
    /// Takes precedence over any model declared in the pipeline YAML.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Override the provider base URL for all runner invocations (e.g. `http://localhost:11434`).
    /// Set as `ANTHROPIC_BASE_URL` in the runner subprocess environment.
    #[arg(long, value_name = "URL")]
    pub provider_url: Option<String>,

    /// Override the provider auth token for all runner invocations (e.g. `ollama`).
    /// Set as `ANTHROPIC_AUTH_TOKEN` in the runner subprocess environment.
    #[arg(long, value_name = "TOKEN")]
    pub provider_token: Option<String>,

    /// Output format for pipeline execution. `json` emits an NDJSON event stream to stdout.
    #[arg(long, value_name = "FORMAT", default_value = "text")]
    pub output_format: OutputFormat,

    /// Include model thinking/reasoning text in --once text output.
    #[arg(long)]
    pub show_thinking: bool,

    /// Include full step response text in --once text output.
    #[arg(long)]
    pub show_responses: bool,

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

        /// Output format: `text` (default) prints human-readable result; `json` emits a single
        /// JSON object: `{"valid":true}` or `{"valid":false,"errors":[{"message":"...","error_type":"..."}]}`.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        output_format: OutputFormat,
    },
    /// Internal: MCP permission bridge. Spawned by Claude CLI to handle tool permission checks.
    /// Not intended for direct use.
    #[command(name = "mcp-bridge", hide = true)]
    McpBridge {
        /// Path to the Unix domain socket where the main ail process is listening.
        #[arg(long)]
        socket: String,
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
        let cli = Cli::try_parse_from(["ail", "materialize", "--out", "/tmp/out.yaml"]).unwrap();
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
        if let Some(Commands::Validate { pipeline, .. }) = cli.command {
            assert_eq!(pipeline, Some(PathBuf::from("/tmp/test.ail.yaml")));
        } else {
            panic!("expected Validate command");
        }
    }

    #[test]
    fn validate_output_format_json_parses() {
        let cli = Cli::try_parse_from(["ail", "validate", "--output-format", "json"]).unwrap();
        if let Some(Commands::Validate { output_format, .. }) = cli.command {
            assert!(matches!(output_format, OutputFormat::Json));
        } else {
            panic!("expected Validate command");
        }
    }

    #[test]
    fn validate_output_format_defaults_to_text() {
        let cli = Cli::try_parse_from(["ail", "validate"]).unwrap();
        if let Some(Commands::Validate { output_format, .. }) = cli.command {
            assert!(matches!(output_format, OutputFormat::Text));
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
        assert!(cli.model.is_none());
        assert!(cli.provider_url.is_none());
        assert!(cli.provider_token.is_none());
        assert!(cli.command.is_none());
        assert!(matches!(cli.output_format, OutputFormat::Text));
    }

    #[test]
    fn output_format_json_parses() {
        let cli = Cli::try_parse_from(["ail", "--output-format", "json"]).unwrap();
        assert!(matches!(cli.output_format, OutputFormat::Json));
    }

    #[test]
    fn output_format_text_parses() {
        let cli = Cli::try_parse_from(["ail", "--output-format", "text"]).unwrap();
        assert!(matches!(cli.output_format, OutputFormat::Text));
    }

    #[test]
    fn output_format_defaults_to_text() {
        let cli = Cli::try_parse_from(["ail", "--once", "hello"]).unwrap();
        assert!(matches!(cli.output_format, OutputFormat::Text));
    }

    #[test]
    fn model_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--model", "gemma3:1b"]).unwrap();
        assert_eq!(cli.model.as_deref(), Some("gemma3:1b"));
    }

    #[test]
    fn show_thinking_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi", "--show-thinking"]).unwrap();
        assert!(cli.show_thinking);
    }

    #[test]
    fn show_responses_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi", "--show-responses"]).unwrap();
        assert!(cli.show_responses);
    }

    #[test]
    fn provider_flags_parse() {
        let cli = Cli::try_parse_from([
            "ail",
            "--provider-url",
            "http://localhost:11434",
            "--provider-token",
            "ollama",
        ])
        .unwrap();
        assert_eq!(cli.provider_url.as_deref(), Some("http://localhost:11434"));
        assert_eq!(cli.provider_token.as_deref(), Some("ollama"));
    }
}
