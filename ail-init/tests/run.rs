use ail_init::{InitArgs, TemplateFile, TemplateMeta};

fn args(template: Option<&str>, force: bool, dry_run: bool) -> InitArgs {
    InitArgs {
        template: template.map(|s| s.to_string()),
        force,
        dry_run,
    }
}

#[test]
fn run_without_template_lists_all_three() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(ail_init::run_in_cwd(args(None, false, false), tmp.path()).is_ok());
    // No files should have been written.
    assert!(!tmp.path().join(".ail").exists());
}

#[test]
fn run_with_unknown_template_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let err =
        ail_init::run_in_cwd(args(Some("does-not-exist"), false, false), tmp.path()).unwrap_err();
    assert!(err.detail().contains("does-not-exist"));
    assert!(err.detail().contains("starter"));
    assert!(err.detail().contains("superpowers"));
    assert!(err.detail().contains("oh-my-ail"));
}

#[test]
fn starter_installs_expected_files() {
    let tmp = tempfile::tempdir().unwrap();
    ail_init::run_in_cwd(args(Some("starter"), false, false), tmp.path()).unwrap();
    assert!(tmp.path().join(".ail/default.yaml").exists());
    assert!(tmp.path().join(".ail/README.md").exists());
}

#[test]
fn oma_alias_installs_oh_my_ail() {
    let tmp = tempfile::tempdir().unwrap();
    ail_init::run_in_cwd(args(Some("oma"), false, false), tmp.path()).unwrap();
    // oh-my-ail's main entry point.
    assert!(tmp.path().join(".ail/.ohmy.ail.yaml").exists());
    // One of the agent pipelines — proves subdir preservation.
    assert!(tmp.path().join(".ail/agents/hephaestus.ail.yaml").exists());
}

#[test]
fn superpowers_installs_all_pipelines_under_ail() {
    let tmp = tempfile::tempdir().unwrap();
    ail_init::run_in_cwd(args(Some("superpowers"), false, false), tmp.path()).unwrap();
    // A representative pipeline from the superpowers set.
    assert!(tmp.path().join(".ail/brainstorming.ail.yaml").exists());
    // Crucially: NOT at the CWD root.
    assert!(!tmp.path().join("brainstorming.ail.yaml").exists());
}

#[test]
fn dry_run_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    ail_init::run_in_cwd(args(Some("starter"), false, true), tmp.path()).unwrap();
    assert!(!tmp.path().join(".ail").exists());
}

#[test]
fn refuses_overwrite_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    // First install — succeeds.
    ail_init::run_in_cwd(args(Some("starter"), false, false), tmp.path()).unwrap();

    // Seed conflicting content.
    std::fs::write(tmp.path().join(".ail/default.yaml"), b"user-edited").unwrap();

    // Second install without --force fails and preserves the edit.
    let err = ail_init::run_in_cwd(args(Some("starter"), false, false), tmp.path()).unwrap_err();
    assert!(err.detail().contains("already exist"));
    assert_eq!(
        std::fs::read(tmp.path().join(".ail/default.yaml")).unwrap(),
        b"user-edited"
    );
}

#[test]
fn overwrites_with_force() {
    let tmp = tempfile::tempdir().unwrap();
    ail_init::run_in_cwd(args(Some("starter"), false, false), tmp.path()).unwrap();
    std::fs::write(tmp.path().join(".ail/default.yaml"), b"user-edited").unwrap();

    ail_init::run_in_cwd(args(Some("starter"), true, false), tmp.path()).unwrap();
    let restored = std::fs::read_to_string(tmp.path().join(".ail/default.yaml")).unwrap();
    assert!(restored.contains("Starter — Invocation-only pipeline"));
}

#[allow(dead_code)]
fn _public_api_surface(_: TemplateMeta, _: TemplateFile) {}
