//! Command trait — lifecycle contract for CLI commands.
//!
//! Every CLI command (validate, materialize, delete, once-text, once-json, chat, etc.)
//! implements this trait. This gives:
//!
//! 1. **Clear contract** for "how do I add a new command?" — implement `Command`.
//! 2. **Lifecycle guarantees** — `teardown()` always runs, even after errors.
//! 3. **Testability** — commands can be tested without CLI parsing.
//! 4. **Extensibility signal** — the trait is the seam for future external commands.
//!
//! # Async
//!
//! `execute()` is async because some commands (once-json, chat) need the tokio runtime.
//! Sync commands simply return immediately without awaiting anything.
//!
//! # Usage
//!
//! ```ignore
//! let mut cmd = ValidateCommand::new(..);
//! cmd.validate()?;
//! let outcome = cmd.execute().await;
//! cmd.teardown();
//! std::process::exit(outcome.exit_code());
//! ```

/// Outcome of a command execution.
pub enum CommandOutcome {
    /// Command completed successfully.
    Success,
    /// Command completed with a specific exit code.
    ExitCode(i32),
}

impl CommandOutcome {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Success => 0,
            Self::ExitCode(code) => *code,
        }
    }
}
