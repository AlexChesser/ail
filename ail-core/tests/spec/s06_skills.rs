/// SPEC §6 — skill: step type and built-in module execution.
use ail_core::config;
use ail_core::error::error_types;
use ail_core::executor;
use ail_core::runner::stub::StubRunner;
use ail_core::session::log_provider::NullProvider;
use ail_core::session::Session;
use ail_core::skill::SkillRegistry;

use std::io::Write;

fn load_yaml(yaml: &str) -> ail_core::config::domain::Pipeline {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.ail.yaml");
    let mut f = std::fs::File::create(&path).expect("create");
    f.write_all(yaml.as_bytes()).expect("write");
    config::load(&path).expect("load")
}

/// A `skill:` step with a valid built-in name parses successfully.
#[test]
fn skill_step_parses_with_builtin_name() {
    let pipeline = load_yaml(
        r#"
version: "1"
pipeline:
  - id: review
    skill: ail/code_review
"#,
    );
    assert_eq!(pipeline.steps.len(), 1);
    assert_eq!(pipeline.steps[0].id.as_str(), "review");
    assert!(matches!(
        pipeline.steps[0].body,
        ail_core::config::domain::StepBody::Skill { .. }
    ));
}

/// A `skill:` step with an unknown name parses but fails at execution time.
#[test]
fn skill_step_parses_with_unknown_name() {
    let pipeline = load_yaml(
        r#"
version: "1"
pipeline:
  - id: mystery
    skill: ail/nonexistent
"#,
    );
    assert_eq!(pipeline.steps.len(), 1);
    // Parsing succeeds — unknown skill detection happens at execution time.
}

/// Executing a built-in skill step invokes the runner with the resolved prompt.
#[test]
fn skill_step_executes_builtin_code_review() {
    let pipeline = load_yaml(
        r#"
version: "1"
pipeline:
  - id: invocation
    prompt: "function add(a, b) { return a + b; }"
  - id: review
    skill: ail/code_review
"#,
    );
    let mut session =
        Session::new(pipeline, "test".to_string()).with_log_provider(Box::new(NullProvider));
    let runner = StubRunner::new("LGTM");
    let result = executor::execute(&mut session, &runner);

    assert!(result.is_ok());
    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].step_id, "review");
    assert_eq!(entries[1].response.as_deref(), Some("LGTM"));
    // The prompt should contain the skill template content, not raw template vars.
    assert!(
        !entries[1].prompt.contains("{{ last_response }}"),
        "template variables should be resolved before runner invocation"
    );
    // The prompt should contain the review instructions.
    assert!(
        entries[1].prompt.contains("code reviewer"),
        "resolved prompt should contain skill instructions"
    );
}

/// Executing an unknown skill step produces a SKILL_UNKNOWN error.
#[test]
fn skill_step_unknown_returns_typed_error() {
    let pipeline = load_yaml(
        r#"
version: "1"
pipeline:
  - id: bad
    skill: ail/does_not_exist
"#,
    );
    let mut session =
        Session::new(pipeline, "test".to_string()).with_log_provider(Box::new(NullProvider));
    let runner = StubRunner::new("unused");
    let result = executor::execute(&mut session, &runner);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::SKILL_UNKNOWN);
    assert!(err.detail().contains("does_not_exist"));
}

/// The built-in skill registry contains at least the documented modules.
#[test]
fn builtin_registry_contains_documented_skills() {
    let registry = SkillRegistry::new();
    let names = registry.list();
    assert!(
        names.contains(&"ail/code_review"),
        "missing ail/code_review"
    );
    assert!(
        names.contains(&"ail/test_writer"),
        "missing ail/test_writer"
    );
    assert!(
        names.contains(&"ail/security_audit"),
        "missing ail/security_audit"
    );
    assert!(names.contains(&"ail/janitor"), "missing ail/janitor");
}

/// Skill steps can use `on_result` branching like any other step.
#[test]
fn skill_step_on_result_branching_works() {
    let pipeline = load_yaml(
        r#"
version: "1"
pipeline:
  - id: invocation
    prompt: "some code"
  - id: audit
    skill: ail/security_audit
    on_result:
      - contains: "VULNERABILITY"
        action: abort_pipeline
"#,
    );
    let mut session =
        Session::new(pipeline, "test".to_string()).with_log_provider(Box::new(NullProvider));
    // Runner returns a response containing "VULNERABILITY" — should trigger abort.
    let runner = StubRunner::new("Found a VULNERABILITY in auth module");
    let result = executor::execute(&mut session, &runner);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::PIPELINE_ABORTED);
}
