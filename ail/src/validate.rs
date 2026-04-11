use crate::cli::OutputFormat;

pub fn handle_validate(pipeline: Option<std::path::PathBuf>, output_format: OutputFormat) {
    let path = match ail_core::config::discovery::discover(pipeline) {
        Some(p) => p,
        None => {
            match output_format {
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
            std::process::exit(1);
        }
    };
    match ail_core::config::load(&path) {
        Ok(p) => match output_format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({"valid": true, "step_count": p.steps.len()})
                );
            }
            OutputFormat::Text => {
                println!("Pipeline valid: {} step(s)", p.steps.len());
            }
        },
        Err(e) => match output_format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::json!({
                        "valid": false,
                        "errors": [{"message": e.detail(), "error_type": e.error_type()}]
                    })
                );
                std::process::exit(1);
            }
            OutputFormat::Text => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
    }
}
