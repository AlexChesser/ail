use crate::command::CommandOutcome;

pub struct MaterializeCommand {
    pipeline: ail_core::config::domain::Pipeline,
    out: Option<std::path::PathBuf>,
    expand_pipelines: bool,
}

impl MaterializeCommand {
    pub fn new(
        pipeline: ail_core::config::domain::Pipeline,
        out: Option<std::path::PathBuf>,
        expand_pipelines: bool,
    ) -> Self {
        Self {
            pipeline,
            out,
            expand_pipelines,
        }
    }

    pub fn execute(&self) -> CommandOutcome {
        let output = if self.expand_pipelines {
            match ail_core::materialize::materialize_expanded(&self.pipeline) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    return CommandOutcome::ExitCode(1);
                }
            }
        } else {
            ail_core::materialize::materialize(&self.pipeline)
        };
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
