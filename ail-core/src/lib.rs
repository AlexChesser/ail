pub mod config;
pub mod error;
pub mod executor;
pub mod materialize;
pub mod runner;
pub mod session;
pub mod template;

pub fn version() -> &'static str {
    "0.0.1"
}
