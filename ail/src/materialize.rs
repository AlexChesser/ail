use crate::command::CommandOutcome;

pub struct MaterializeCommand {
    pipeline: ail_core::config::domain::Pipeline,
    out: Option<std::path::PathBuf>,
}

impl MaterializeCommand {
    pub fn new(
        pipeline: ail_core::config::domain::Pipeline,
        out: Option<std::path::PathBuf>,
    ) -> Self {
        Self { pipeline, out }
    }

    pub fn execute(&self) -> CommandOutcome {
        let output = ail_core::materialize::materialize(&self.pipeline);
        match &self.out {
            Some(out_path) => {
                if let Err(e) = std::fs::write(out_path, &output) {
                    eprintln!("Failed to write to {}: {e}", out_path.display());
                    return CommandOutcome::ExitCode(1);
                }
            }
            None => print!("{output}"),
        }
        CommandOutcome::Success
    }
}
