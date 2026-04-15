//! End-to-end tests for `ail materialize` (alias: `mp`).

mod common;

#[test]
fn materialize_prints_to_stdout() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["materialize", "--pipeline"])
        .arg(common::fixture_path("minimal.ail.yaml"));

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("version:"));
}

#[test]
fn materialize_writes_file_when_out_given() {
    let (mut cmd, home) = common::ail_cmd_isolated();
    let out_path = home.path().join("out.yaml");
    cmd.args(["materialize", "--pipeline"])
        .arg(common::fixture_path("minimal.ail.yaml"))
        .arg("--out")
        .arg(&out_path);

    cmd.assert().success();

    assert!(out_path.exists(), "Output file should be created");
    let content = std::fs::read_to_string(&out_path).expect("should read output file");
    assert!(
        content.contains("version:"),
        "Output should contain 'version:', got: {content}"
    );
}

#[test]
fn materialize_mp_alias_works() {
    let (mut cmd, _home) = common::ail_cmd_isolated();
    cmd.args(["mp", "--pipeline"])
        .arg(common::fixture_path("minimal.ail.yaml"));

    cmd.assert()
        .success()
        .stdout(predicates::str::contains("version:"));
}
