use crate::cli::OutputFormat;
use crate::command::CommandOutcome;

pub struct ValidateCommand {
    pipeline_path: Option<std::path::PathBuf>,
    output_format: OutputFormat,
}

impl ValidateCommand {
    pub fn new(pipeline_path: Option<std::path::PathBuf>, output_format: OutputFormat) -> Self {
        Self {
            pipeline_path,
            output_format,
        }
    }

    pub fn execute(&self) -> CommandOutcome {
        let path = match ail_core::config::discovery::discover(self.pipeline_path.clone()) {
            Some(p) => p,
            None => {
                match self.output_format {
                    OutputFormat::Json => {
                        println!(
                            "{}",
                            serde_json::json!({
                                "valid": false,
                                "errors": [{"message": "No pipeline file found.", "error_type": "ail:config/file-not-found"}]
                            })
                        );
                    }
                    OutputFormat::Text => {
                        eprintln!("No pipeline file found.");
                    }
                }
                return CommandOutcome::ExitCode(1);
            }
        };
        match ail_core::config::load(&path) {
            Ok(p) => {
                match self.output_format {
                    OutputFormat::Json => {
                        println!(
                            "{}",
                            serde_json::json!({"valid": true, "step_count": p.steps.len()})
                        );
                    }
                    OutputFormat::Text => {
                        println!("Pipeline valid: {} step(s)", p.steps.len());
                    }
                }
                CommandOutcome::Success
            }
            Err(e) => {
                match self.output_format {
                    OutputFormat::Json => {
                        println!(
                            "{}",
                            serde_json::json!({
                                "valid": false,
                                "errors": [{"message": e.detail(), "error_type": e.error_type()}]
                            })
                        );
                    }
                    OutputFormat::Text => {
                        eprintln!("{e}");
                    }
                }
                CommandOutcome::ExitCode(1)
            }
        }
    }
}
