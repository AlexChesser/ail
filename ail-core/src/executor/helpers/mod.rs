//! Shared helper functions used by both the headless and controlled execution paths.

pub(super) mod condition;
mod invocation;
mod on_result;
mod runner_resolution;
mod shell;
mod system_prompt;

// Public re-export (used by executor/mod.rs)
pub use invocation::run_invocation_step;

// Crate-internal re-exports (used by core.rs and dispatch modules)
pub(super) use condition::evaluate_condition;
pub(super) use on_result::{build_tool_policy, evaluate_on_result};
pub(super) use runner_resolution::{
    build_step_runner_box, resolve_effective_runner_name, resolve_step_provider,
};
pub(super) use shell::run_shell_command;
pub(super) use system_prompt::{resolve_prompt_file, resolve_step_system_prompts};
