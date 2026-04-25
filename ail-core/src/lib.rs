pub mod config;
pub mod delete;
pub mod error;
pub mod executor;
pub mod formatter;
pub mod fs_util;
pub mod ipc;
pub mod logs;
pub mod materialize;
pub mod protocol;
pub mod runner;
pub mod session;
pub mod skill;
pub mod template;

#[doc(hidden)]
pub mod test_helpers;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn version_full() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        " (rev ",
        env!("VERGEN_GIT_SHA"),
        ", built ",
        env!("VERGEN_BUILD_DATE"),
        ")"
    )
}
