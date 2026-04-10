pub mod config;
pub mod delete;
pub mod error;
pub mod executor;
pub mod formatter;
pub mod ipc;
pub mod logs;
pub mod materialize;
pub mod protocol;
pub mod runner;
pub mod session;
pub mod template;

#[doc(hidden)]
pub mod test_helpers;

pub fn version() -> &'static str {
    "0.0.1"
}
