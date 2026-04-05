//! Handler for the `ail delete` subcommand.
#![allow(clippy::result_large_err)]

use ail_core::delete::delete_run;
use ail_core::error::AilError;

/// Execute the delete command.
///
/// Parameters are passed via the run_id and force/json flags.
#[allow(clippy::result_large_err)]
pub fn handle_delete(run_id: String, force: bool, json: bool) -> Result<(), AilError> {
    delete_run(&run_id, force)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "deleted": true,
                "run_id": run_id,
                "message": format!("Deleted run {}", run_id)
            })
        );
    } else {
        println!("Deleted run {}", run_id);
    }

    Ok(())
}
