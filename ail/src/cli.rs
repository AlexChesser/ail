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
    version = ail_core::version_full(),
    about = "Artificial Intelligence Loops — the control plane for how agents behave after the human stops typing."
)]
pub struct Cli {
    /// Prompt to run (positional — canonical form). Equivalent to `--once <PROMPT>`.
    #[arg(value_name = "PROMPT", conflicts_with = "once")]
    pub prompt: Option<String>,

    /// Run a single non-interactive prompt and exit (long-form alias for the positional argument).
    #[arg(long, value_name = "PROMPT")]
    pub once: Option<String>,

    /// Override pipeline file discovery (default: .ail.yaml → .ail/default.yaml → ~/.config/ail/default.yaml)
    #[arg(long, value_name = "PATH")]
    pub pipeline: Option<PathBuf>,

    /// Skip all tool-use permission prompts (passes --dangerously-skip-permissions to the runner)
    #[arg(long)]
    pub headless: bool,

    /// Override the model for all runner invocations
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

    /// Output format: text (default) or json (NDJSON event stream)
    #[arg(long, value_name = "FORMAT", default_value = "text")]
    pub output_format: OutputFormat,

    /// Include model thinking/reasoning text in text output.
    #[arg(long)]
    pub show_thinking: bool,

    /// Print one summary line per completed step
    #[arg(long)]
    pub show_work: bool,

    /// Resolve and print the pipeline without making any LLM calls
    #[arg(long)]
    pub dry_run: bool,

    /// Stream per-step progress as it arrives
    #[arg(long, alias = "show-responses", hide_short_help = false)]
    pub watch: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Print the fully-resolved pipeline YAML with all template variables resolved
    #[command(name = "materialize", visible_alias = "mp")]
    Materialize {
        /// Path to the pipeline YAML file. Overrides automatic discovery.
        #[arg(long, value_name = "PATH")]
        pipeline: Option<PathBuf>,

        /// Write output to a file instead of stdout.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,

        /// Expand named pipeline references inline (SPEC §10, §17).
        #[arg(long)]
        expand_pipelines: bool,
    },
    /// Check a pipeline file for errors without running it
    Validate {
        /// Path to the pipeline YAML file to validate. Overrides automatic discovery.
        #[arg(long, value_name = "PATH")]
        pipeline: Option<PathBuf>,

        /// Output format: `text` (default) prints human-readable result; `json` emits a single
        /// JSON object: `{"valid":true}` or `{"valid":false,"errors":[{"message":"...","error_type":"..."}]}`.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        output_format: OutputFormat,
    },
    /// Query recorded pipeline runs
    Logs {
        /// Filter by session run_id (prefix match).
        #[arg(long)]
        session: Option<String>,

        /// Full-text search across step content.
        #[arg(long)]
        query: Option<String>,

        /// Output format: `text` (default) or `json` (NDJSON, one object per session).
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,

        /// Stream new log entries in real-time (poll every second).
        #[arg(long)]
        tail: bool,

        /// Maximum number of sessions to return.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Show a single run in full detail
    Log {
        /// Run ID (UUID). If omitted, shows the most recent run for the current directory.
        #[arg(value_name = "RUN_ID")]
        run_id: Option<String>,

        /// Output format: `markdown` (default, ail-log/1), `json` (NDJSON), or `raw` (stored JSONL).
        #[arg(long, value_name = "FORMAT", default_value = "markdown")]
        format: String,

        /// Stream new events as they arrive (500ms polling). Exits when run completes.
        #[arg(long)]
        follow: bool,
    },
    /// Internal: PreToolUse hook for AskUserQuestion. Spawned by Claude CLI.
    /// Not intended for direct use.
    #[command(name = "ask-user-hook", hide = true)]
    AskUserHook {
        /// Path to the Unix domain socket where the main ail process is listening.
        #[arg(long)]
        socket: String,
    },
    /// Internal: PreToolUse hook for tool permission checks. Spawned by Claude CLI.
    /// Not intended for direct use.
    #[command(name = "check-permission-hook", hide = true)]
    CheckPermissionHook {
        /// Path to the Unix domain socket where the main ail process is listening.
        #[arg(long)]
        socket: String,
    },
    /// Machine-facing bidirectional NDJSON protocol over stdin/stdout
    Stdio {
        /// Send a single message and exit (non-interactive).
        #[arg(long, short)]
        message: Option<String>,
        /// Stream NDJSON events to stdout.
        #[arg(long)]
        stream: bool,
        /// Path to the pipeline YAML file. Overrides automatic discovery.
        #[arg(long, value_name = "PATH")]
        pipeline: Option<PathBuf>,
        /// Override the model for all runner invocations.
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
        /// Override the provider base URL.
        #[arg(long, value_name = "URL")]
        provider_url: Option<String>,
        /// Override the provider auth token.
        #[arg(long, value_name = "TOKEN")]
        provider_token: Option<String>,
    },
    /// Browse the embedded AIL specification
    #[command(long_about = "Browse the embedded AIL specification.\n\n\
                            By default, prints the full specification in prose form.\n\n\
                            Progressive disclosure options:\n\
                            \n\
                            --list                  Print a table of contents with section IDs and word counts\n\
                            --section s05           Print a single section by ID\n\
                            --section s05,r02       Print multiple sections (comma-separated)\n\
                            --format compact        Authoring reference — everything an LLM needs to write a correct .ail.yaml,\n\
                                                    with ail's internals, roadmap, and operational tooling stripped (~40-50k tokens)\n\
                            --format schema         Annotated YAML schema (~2-3k tokens)\n\
                            --core / --runner       Filter to core spec or runner spec only\n\n\
                            Section IDs: s01–s34 (core), r01–r11 (runner). Run `ail spec --list` to browse.")]
    Spec {
        /// Output format: `prose` (default, full spec), `compact` (authoring reference for LLMs),
        /// or `schema` (annotated YAML schema).
        #[arg(long, default_value = "prose")]
        format: String,

        /// Print specific section(s) by ID (e.g. `s05`, `r02`). Comma-separated for multiple.
        #[arg(long, value_delimiter = ',')]
        section: Option<Vec<String>>,

        /// List available section IDs with titles and word counts.
        #[arg(long)]
        list: bool,

        /// Print only core spec sections (s-prefixed).
        #[arg(long)]
        core: bool,

        /// Print only runner spec sections (r-prefixed).
        #[arg(long)]
        runner: bool,
    },
    /// Scaffold an ail workspace from a template
    Init {
        /// Template name or alias. If omitted, shows an interactive picker.
        #[arg(value_name = "TEMPLATE")]
        template: Option<String>,

        /// Overwrite existing files without prompting.
        #[arg(long)]
        force: bool,

        /// Show which files would be written without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print a CLAUDE.md snippet that teaches an agent how to use ail in this project
    #[command(
        name = "agent-guide",
        long_about = "Print a CLAUDE.md (or AGENTS.md) snippet that teaches an agent how \
                      to use ail in this project.\n\n\
                      The snippet is short, self-contained, and points at \
                      `ail spec --format compact` as the canonical authoring \
                      reference rather than restating spec content. Pipe it \
                      into your project's CLAUDE.md or AGENTS.md:\n\n\
                      \tail agent-guide >> CLAUDE.md\n\n\
                      Re-run after upgrading ail to refresh the snippet."
    )]
    AgentGuide {
        /// Output format. `claudemd` (default) is markdown valid for CLAUDE.md and AGENTS.md.
        #[arg(long, default_value = "claudemd")]
        format: String,
    },
    /// Delete a recorded run from history
    Delete {
        /// Run ID to delete.
        #[arg(value_name = "RUN_ID")]
        run_id: String,

        /// Skip validation — delete even if the JSONL file is missing.
        #[arg(long)]
        force: bool,

        /// Output result as JSON.
        #[arg(long)]
        json: bool,
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
    fn positional_prompt_parses() {
        let cli = Cli::try_parse_from(["ail", "hello world"]).unwrap();
        assert_eq!(cli.prompt.as_deref(), Some("hello world"));
    }

    #[test]
    fn positional_and_once_conflict() {
        let result = Cli::try_parse_from(["ail", "hello", "--once", "world"]);
        assert!(result.is_err());
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
    fn materialize_expand_pipelines_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "materialize", "--expand-pipelines"]).unwrap();
        if let Some(Commands::Materialize {
            expand_pipelines, ..
        }) = cli.command
        {
            assert!(expand_pipelines);
        } else {
            panic!("expected Materialize command");
        }
    }

    #[test]
    fn materialize_expand_pipelines_defaults_to_false() {
        let cli = Cli::try_parse_from(["ail", "materialize"]).unwrap();
        if let Some(Commands::Materialize {
            expand_pipelines, ..
        }) = cli.command
        {
            assert!(!expand_pipelines);
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
    fn init_subcommand_parses() {
        let cli = Cli::try_parse_from(["ail", "init"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Init { .. })));
    }

    #[test]
    fn init_with_template_parses() {
        let cli = Cli::try_parse_from(["ail", "init", "starter"]).unwrap();
        if let Some(Commands::Init { template, .. }) = cli.command {
            assert_eq!(template.as_deref(), Some("starter"));
        } else {
            panic!("expected Init command");
        }
    }

    #[test]
    fn init_force_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "init", "starter", "--force"]).unwrap();
        if let Some(Commands::Init { force, .. }) = cli.command {
            assert!(force);
        } else {
            panic!("expected Init command");
        }
    }

    #[test]
    fn init_dry_run_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "init", "--dry-run"]).unwrap();
        if let Some(Commands::Init { dry_run, .. }) = cli.command {
            assert!(dry_run);
        } else {
            panic!("expected Init command");
        }
    }

    #[test]
    fn init_defaults_to_no_template_no_force_no_dry_run() {
        let cli = Cli::try_parse_from(["ail", "init"]).unwrap();
        if let Some(Commands::Init {
            template,
            force,
            dry_run,
        }) = cli.command
        {
            assert!(template.is_none());
            assert!(!force);
            assert!(!dry_run);
        } else {
            panic!("expected Init command");
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
        assert!(cli.prompt.is_none());
        assert!(cli.pipeline.is_none());
        assert!(!cli.headless);
        assert!(cli.model.is_none());
        assert!(cli.provider_url.is_none());
        assert!(cli.provider_token.is_none());
        assert!(cli.command.is_none());
        assert!(matches!(cli.output_format, OutputFormat::Text));
        assert!(!cli.dry_run);
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
    fn watch_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi", "--watch"]).unwrap();
        assert!(cli.watch);
    }

    #[test]
    fn show_responses_alias_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi", "--show-responses"]).unwrap();
        assert!(cli.watch);
    }

    #[test]
    fn show_work_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi", "--show-work"]).unwrap();
        assert!(cli.show_work);
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

    #[test]
    fn dry_run_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi", "--dry-run"]).unwrap();
        assert!(cli.dry_run);
    }

    #[test]
    fn dry_run_flag_defaults_to_false() {
        let cli = Cli::try_parse_from(["ail", "--once", "hi"]).unwrap();
        assert!(!cli.dry_run);
    }

    #[test]
    fn agent_guide_subcommand_parses() {
        let cli = Cli::try_parse_from(["ail", "agent-guide"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::AgentGuide { .. })));
    }

    #[test]
    fn agent_guide_format_defaults_to_claudemd() {
        let cli = Cli::try_parse_from(["ail", "agent-guide"]).unwrap();
        if let Some(Commands::AgentGuide { format }) = cli.command {
            assert_eq!(format, "claudemd");
        } else {
            panic!("expected AgentGuide command");
        }
    }

    #[test]
    fn agent_guide_format_flag_parses() {
        let cli = Cli::try_parse_from(["ail", "agent-guide", "--format", "agents-md"]).unwrap();
        if let Some(Commands::AgentGuide { format }) = cli.command {
            assert_eq!(format, "agents-md");
        } else {
            panic!("expected AgentGuide command");
        }
    }
}
