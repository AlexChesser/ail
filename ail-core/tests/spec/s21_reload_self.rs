//! SPEC §21 — `action: reload_self` pipeline hot-reload primitive.
//!
//! Covers the minimum self-modification primitive: a running pipeline re-reads
//! its own `.ail.yaml` on disk, swaps the pipeline in place, and re-anchors the
//! top-level executor loop by step-id.

use ail_core::config::domain::{ActionKind, Step, StepBody, StepId, MAX_RELOADS_PER_RUN};
use ail_core::error::error_types;
use ail_core::executor::execute;
use ail_core::runner::stub::StubRunner;
use ail_core::session::log_provider::NullProvider;
use ail_core::session::Session;

fn write_yaml(dir: &std::path::Path, filename: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, contents).expect("write pipeline fixture");
    path
}

fn load_session(path: &std::path::Path) -> Session {
    let pipeline = ail_core::config::load(path).expect("load pipeline fixture");
    Session::new(pipeline, "invocation prompt".to_string())
        .with_log_provider(Box::new(NullProvider))
}

/// When the on-disk pipeline is unchanged, `reload_self` is a no-op aside from
/// its own turn-log entry; subsequent steps still execute.
#[test]
fn reload_self_noop_continues() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_yaml(
        tmp.path(),
        ".ail.yaml",
        r#"version: "1"
pipeline:
  - id: before
    prompt: "step before reload"
  - id: reload
    action: reload_self
  - id: after
    prompt: "step after reload"
"#,
    );

    let mut session = load_session(&path);
    let runner = StubRunner::new("stub");

    execute(&mut session, &runner).expect("pipeline ok");

    let ids: Vec<&str> = session
        .turn_log
        .entries()
        .iter()
        .map(|e| e.step_id.as_str())
        .collect();
    assert_eq!(ids, vec!["before", "reload", "after"]);

    let reload_entry = &session.turn_log.entries()[1];
    assert_eq!(reload_entry.prompt, "reload_self");
    let resp = reload_entry.response.as_deref().unwrap_or_default();
    assert!(
        resp.starts_with("reloaded pipeline (3 -> 3 steps)"),
        "unexpected reload response: {resp}"
    );
    assert_eq!(session.reload_count, 1);
    assert!(!session.reload_requested);
}

/// When the YAML is rewritten before reload_self fires, the new tail steps run
/// in the current invocation — the whole point of the primitive.
#[test]
fn reload_self_picks_up_new_step_appended_to_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_yaml(
        tmp.path(),
        ".ail.yaml",
        r#"version: "1"
pipeline:
  - id: rewrite
    prompt: "placeholder — in a real run this step's LLM would edit the file"
  - id: reload
    action: reload_self
"#,
    );

    let mut session = load_session(&path);

    // Simulate the effect of the `rewrite` step: overwrite the YAML on disk to
    // append a tail step. In a real run the LLM's Write/Edit tool would do this.
    std::fs::write(
        &path,
        r#"version: "1"
pipeline:
  - id: rewrite
    prompt: "placeholder"
  - id: reload
    action: reload_self
  - id: newly_added
    prompt: "added by rewrite step"
"#,
    )
    .unwrap();

    let runner = StubRunner::new("stub");
    execute(&mut session, &runner).expect("pipeline ok");

    let ids: Vec<&str> = session
        .turn_log
        .entries()
        .iter()
        .map(|e| e.step_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["rewrite", "reload", "newly_added"],
        "the reloaded tail step must execute in the same run"
    );

    let reload_entry = &session.turn_log.entries()[1];
    let resp = reload_entry.response.as_deref().unwrap_or_default();
    assert!(
        resp.contains("2 -> 3 steps"),
        "reload entry should report step-count delta, got: {resp}"
    );
}

/// Passthrough pipelines have no `source` — reload_self must abort cleanly
/// with a typed error rather than panic.
#[test]
fn reload_self_in_passthrough_errors() {
    let reload_step = Step {
        id: StepId("reload".to_string()),
        body: StepBody::Action(ActionKind::ReloadSelf),
        ..Default::default()
    };
    let mut session = Session::new(
        ail_core::config::domain::Pipeline {
            steps: vec![reload_step],
            ..Default::default()
        },
        "prompt".to_string(),
    )
    .with_log_provider(Box::new(NullProvider));

    let runner = StubRunner::new("unused");
    let err = execute(&mut session, &runner).expect_err("reload without source must fail");
    assert_eq!(err.error_type(), error_types::PIPELINE_RELOAD_FAILED);
    assert!(err.detail().contains("passthrough"));
}

/// Once the per-run reload cap is hit, further reloads fail fast — guard
/// against an LLM driving an infinite self-rewrite loop.
#[test]
fn reload_self_cap_aborts() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_yaml(
        tmp.path(),
        ".ail.yaml",
        r#"version: "1"
pipeline:
  - id: reload
    action: reload_self
"#,
    );

    let mut session = load_session(&path);
    // Simulate a run that has already hit the cap.
    session.reload_count = MAX_RELOADS_PER_RUN;

    let runner = StubRunner::new("unused");
    let err = execute(&mut session, &runner).expect_err("reload beyond cap must fail");
    assert_eq!(err.error_type(), error_types::PIPELINE_RELOAD_FAILED);
    assert!(err.detail().contains("reload cap"));
}

/// If the reloaded pipeline no longer contains the reload step's own id, the
/// executor cannot pick a safe resume point and must abort.
#[test]
fn reload_self_missing_anchor_aborts() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_yaml(
        tmp.path(),
        ".ail.yaml",
        r#"version: "1"
pipeline:
  - id: reload
    action: reload_self
"#,
    );

    let mut session = load_session(&path);

    // Rewrite the file so the anchor id "reload" disappears.
    std::fs::write(
        &path,
        r#"version: "1"
pipeline:
  - id: something_else
    prompt: "no reload step here"
"#,
    )
    .unwrap();

    let runner = StubRunner::new("unused");
    let err = execute(&mut session, &runner).expect_err("missing anchor must fail");
    assert_eq!(err.error_type(), error_types::PIPELINE_RELOAD_FAILED);
    assert!(
        err.detail().contains("reload"),
        "error should reference the reload step: {}",
        err.detail()
    );
}

/// If the rewritten YAML is syntactically invalid, reload fails via the typed
/// reload error (not a panic, not a silent skip).
#[test]
fn reload_self_invalid_yaml_aborts() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_yaml(
        tmp.path(),
        ".ail.yaml",
        r#"version: "1"
pipeline:
  - id: reload
    action: reload_self
"#,
    );

    let mut session = load_session(&path);

    std::fs::write(&path, "{ broken yaml: : : }").unwrap();

    let runner = StubRunner::new("unused");
    let err = execute(&mut session, &runner).expect_err("invalid reload must fail");
    assert_eq!(err.error_type(), error_types::PIPELINE_RELOAD_FAILED);
}
