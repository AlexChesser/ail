//! Handler for the `ail delete` subcommand.
#![allow(clippy::result_large_err)]

use crate::command::CommandOutcome;

pub struct DeleteCommand {
    run_id: String,
    force: bool,
    json: bool,
}

impl DeleteCommand {
    pub fn new(run_id: String, force: bool, json: bool) -> Self {
        Self {
            run_id,
            force,
            json,
        }
    }

    pub fn execute(&self) -> CommandOutcome {
        if let Err(e) = ail_core::delete::delete_run(&self.run_id, self.force) {
            if self.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "deleted": false,
                        "run_id": self.run_id,
                        "error": e.detail(),
                    })
                );
            } else {
                eprintln!("{e}");
            }
            return CommandOutcome::ExitCode(1);
        }

        if self.json {
            println!(
                "{}",
                serde_json::json!({
                    "deleted": true,
                    "run_id": self.run_id,
                    "message": format!("Deleted run {}", self.run_id)
                })
            );
        } else {
            println!("Deleted run {}", self.run_id);
        }

        CommandOutcome::Success
    }
}
