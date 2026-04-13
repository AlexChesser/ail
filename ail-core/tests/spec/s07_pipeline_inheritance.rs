//! SPEC §7 — Pipeline Inheritance (FROM) and Hook Operations
//!
//! Tests the FROM inheritance mechanism, hook operations (run_before, run_after,
//! override, disable), circular inheritance detection, and defaults merging.

use ail_core::config;
use ail_core::config::domain::StepBody;
use ail_core::error::error_types;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ── Basic FROM inheritance ──────────────────────────────────────────────────

/// Pipelines can declare `FROM: <path>` to inherit steps from a base pipeline.
#[test]
fn from_inherits_base_steps() {
    let pipeline = config::load(&fixtures_dir().join("from_plain_child.ail.yaml"))
        .expect("should load child with FROM");
    let ids: Vec<&str> = pipeline.steps.iter().map(|s| s.id.as_str()).collect();
    // Base steps come first, then child's plain steps.
    assert!(
        ids.contains(&"security_audit"),
        "missing base step security_audit"
    );
    assert!(
        ids.contains(&"test_writer"),
        "missing base step test_writer"
    );
    assert!(
        ids.contains(&"commit_checkpoint"),
        "missing base step commit_checkpoint"
    );
    assert!(ids.contains(&"extra_step"), "missing child step extra_step");
    // Order: base steps in order, then child plain steps.
    assert_eq!(
        ids,
        vec![
            "security_audit",
            "test_writer",
            "commit_checkpoint",
            "extra_step"
        ]
    );
}

/// Child defaults override base defaults, but absent child fields fall through.
#[test]
fn from_child_defaults_override_base() {
    let pipeline = config::load(&fixtures_dir().join("from_child.ail.yaml"))
        .expect("should load child with FROM");
    // Child declares model: child-model, overriding base's base-model.
    assert_eq!(pipeline.defaults.model.as_deref(), Some("child-model"));
    // Child does not declare timeout_seconds, so base's 60 falls through.
    assert_eq!(pipeline.timeout_seconds, Some(60));
}

// ── Hook operations ─────────────────────────────────────────────────────────

/// run_before: inserts a step immediately before the named target step.
#[test]
fn hook_run_before_inserts_step() {
    let pipeline = config::load(&fixtures_dir().join("from_child.ail.yaml"))
        .expect("should load child with hooks");
    let ids: Vec<&str> = pipeline.steps.iter().map(|s| s.id.as_str()).collect();
    let sec_pos = ids.iter().position(|id| *id == "security_audit").unwrap();
    let lic_pos = ids
        .iter()
        .position(|id| *id == "license_header_check")
        .unwrap();
    assert!(
        lic_pos < sec_pos,
        "license_header_check should be before security_audit"
    );
    assert_eq!(
        lic_pos + 1,
        sec_pos,
        "license_header_check should be immediately before security_audit"
    );
}

/// run_after: inserts a step immediately after the named target step.
#[test]
fn hook_run_after_inserts_step() {
    let pipeline = config::load(&fixtures_dir().join("from_child.ail.yaml"))
        .expect("should load child with hooks");
    let ids: Vec<&str> = pipeline.steps.iter().map(|s| s.id.as_str()).collect();
    let tw_pos = ids.iter().position(|id| *id == "test_writer").unwrap();
    let cov_pos = ids
        .iter()
        .position(|id| *id == "coverage_reminder")
        .unwrap();
    assert!(
        cov_pos > tw_pos,
        "coverage_reminder should be after test_writer"
    );
    assert_eq!(
        tw_pos + 1,
        cov_pos,
        "coverage_reminder should be immediately after test_writer"
    );
}

/// override: replaces the named step's body, keeping the same id.
#[test]
fn hook_override_replaces_step() {
    let pipeline = config::load(&fixtures_dir().join("from_child.ail.yaml"))
        .expect("should load child with hooks");
    let commit_step = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "commit_checkpoint")
        .expect("commit_checkpoint should still exist");
    match &commit_step.body {
        StepBody::Prompt(text) => {
            assert!(
                text.contains("conventional commits"),
                "override should have replaced the prompt"
            );
        }
        _ => panic!("commit_checkpoint should be a prompt step"),
    }
}

/// disable: removes the named step entirely.
#[test]
fn hook_disable_removes_step() {
    let pipeline = config::load(&fixtures_dir().join("from_disable.ail.yaml"))
        .expect("should load child with disable hook");
    let ids: Vec<&str> = pipeline.steps.iter().map(|s| s.id.as_str()).collect();
    assert!(
        !ids.contains(&"commit_checkpoint"),
        "commit_checkpoint should have been disabled"
    );
    assert_eq!(ids.len(), 2, "should have 2 steps remaining");
    assert_eq!(ids, vec!["security_audit", "test_writer"]);
}

// ── Error conditions ────────────────────────────────────────────────────────

/// Circular inheritance is detected and raises a typed error.
#[test]
fn circular_inheritance_is_detected() {
    let err = config::load(&fixtures_dir().join("from_circular_a.ail.yaml"))
        .expect_err("should detect circular inheritance");
    assert_eq!(err.error_type(), error_types::CIRCULAR_INHERITANCE);
    assert!(
        err.detail().contains("circular inheritance detected"),
        "error detail should mention circular inheritance: {}",
        err.detail()
    );
}

/// Hook targeting a nonexistent step ID raises a validation error.
#[test]
fn hook_targeting_nonexistent_id_is_error() {
    let err = config::load(&fixtures_dir().join("from_invalid_hook_target.ail.yaml"))
        .expect_err("should fail");
    assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    assert!(
        err.detail().contains("nonexistent_step"),
        "error should mention the bad target: {}",
        err.detail()
    );
}

// ── Full materialized step order ────────────────────────────────────────────

/// The complete step order after all hooks are applied matches the expected onion model.
#[test]
fn full_materialized_step_order() {
    let pipeline = config::load(&fixtures_dir().join("from_child.ail.yaml"))
        .expect("should load child with all hooks");
    let ids: Vec<&str> = pipeline.steps.iter().map(|s| s.id.as_str()).collect();
    // Expected order:
    // 1. license_header_check (run_before: security_audit)
    // 2. security_audit (from base)
    // 3. test_writer (from base)
    // 4. coverage_reminder (run_after: test_writer)
    // 5. commit_checkpoint (overridden from base)
    assert_eq!(
        ids,
        vec![
            "license_header_check",
            "security_audit",
            "test_writer",
            "coverage_reminder",
            "commit_checkpoint"
        ]
    );
}

/// A pipeline without FROM still works normally.
#[test]
fn pipeline_without_from_works_normally() {
    let pipeline = config::load(&fixtures_dir().join("solo_developer.ail.yaml"))
        .expect("should load pipeline without FROM");
    assert_eq!(pipeline.steps.len(), 2);
}

/// Hook operations in a pipeline without FROM raise a clear validation error.
#[test]
fn hook_without_from_is_validation_error() {
    let err = config::load(&fixtures_dir().join("from_hook_without_from.ail.yaml"))
        .expect_err("should fail");
    assert_eq!(err.error_type(), error_types::CONFIG_VALIDATION_FAILED);
    assert!(
        err.detail().contains("hook operation"),
        "error should mention hook operation: {}",
        err.detail()
    );
    assert!(
        err.detail().contains("FROM"),
        "error should mention FROM: {}",
        err.detail()
    );
}
