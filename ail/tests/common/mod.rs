//! Shared helpers for ail binary integration tests.
//!
//! Each integration test file is compiled as its own crate — `#[allow(dead_code)]`
//! silences the "never used" warnings that appear in targets that don't happen
//! to use every helper here.

#![allow(dead_code)]

use assert_cmd::Command;
use std::path::PathBuf;
use tempfile::TempDir;

/// Returns the absolute path to a fixture file in `ail/tests/fixtures/`.
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Creates a temporary directory to use as HOME, isolating session/DB state.
pub fn isolated_home() -> TempDir {
    let tmp = TempDir::new().expect("failed to create temp dir");
    // Create the .ail subdirectory so the binary can write session data
    std::fs::create_dir_all(tmp.path().join(".ail")).expect("failed to create .ail dir");
    tmp
}

/// Returns an `assert_cmd::Command` preconfigured for the `ail` binary with:
/// - `AIL_DEFAULT_RUNNER=stub` (so no real Claude/Ollama process is needed)
/// - `HOME` set to the given `home_dir` (isolates sessions and DB)
pub fn ail_cmd(home_dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("ail").expect("ail binary should exist");
    cmd.env("AIL_DEFAULT_RUNNER", "stub");
    cmd.env("HOME", home_dir);
    // Prevent pipeline auto-discovery from picking up the repo's own .ail.yaml
    cmd.current_dir(home_dir);
    cmd
}

/// Convenience: `ail_cmd` with a fresh isolated home.
/// Returns (Command, TempDir) — caller must keep TempDir alive.
pub fn ail_cmd_isolated() -> (Command, TempDir) {
    let home = isolated_home();
    let cmd = ail_cmd(home.path());
    (cmd, home)
}
