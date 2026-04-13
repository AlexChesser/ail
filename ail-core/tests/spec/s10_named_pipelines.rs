/// SPEC §10 — Named pipelines: definition, reference, execution, and circular detection.
use ail_core::config::domain::{Pipeline, Step, StepBody, StepId};
use ail_core::error::error_types;
use ail_core::executor::execute;
use ail_core::runner::stub::{EchoStubRunner, StubRunner};
use ail_core::session::Session;
use std::collections::HashMap;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn make_named_pipeline_step(id: &str, name: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::NamedPipeline {
            name: name.to_string(),
            prompt: None,
        },
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
    }
}

fn prompt_step(id: &str, text: &str) -> Step {
    Step {
        id: StepId(id.to_string()),
        body: StepBody::Prompt(text.to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
        on_error: None,
        before: vec![],
        then: vec![],
        output_schema: None,
    }
}

// ── §10.1 Named pipeline definitions load from YAML ─────────────────────────

#[test]
fn named_pipelines_load_from_yaml() {
    let path = fixtures_dir().join("named_pipelines.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    assert_eq!(pipeline.named_pipelines.len(), 2);
    assert!(pipeline.named_pipelines.contains_key("security_gates"));
    assert!(pipeline.named_pipelines.contains_key("quality_check"));
    // security_gates has 2 steps
    assert_eq!(pipeline.named_pipelines["security_gates"].len(), 2);
    // quality_check has 1 step
    assert_eq!(pipeline.named_pipelines["quality_check"].len(), 1);
}

#[test]
fn pipeline_step_referencing_named_pipeline_becomes_named_pipeline_body() {
    let path = fixtures_dir().join("named_pipelines.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    // run_security references "security_gates" — should be NamedPipeline
    let run_security = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "run_security")
        .expect("run_security step should exist");
    assert!(
        matches!(run_security.body, StepBody::NamedPipeline { ref name, .. } if name == "security_gates"),
        "Expected NamedPipeline body, got: {:?}",
        run_security.body
    );
}

// ── §10.2 Named pipeline execution ──────────────────────────────────────────

#[test]
fn named_pipeline_step_executes_child_steps() {
    let named_steps = vec![
        prompt_step("inner_a", "inner step a"),
        prompt_step("inner_b", "inner step b"),
    ];
    let mut named_pipelines = HashMap::new();
    named_pipelines.insert("my_pipeline".to_string(), named_steps);

    let pipeline = Pipeline {
        steps: vec![make_named_pipeline_step("call_named", "my_pipeline")],
        source: None,
        defaults: Default::default(),
        timeout_seconds: None,
        default_tools: None,
        named_pipelines,
    };
    let mut session = Session::new(pipeline, "invocation prompt".to_string())
        .with_log_provider(Box::new(ail_core::session::log_provider::NullProvider));

    let runner = StubRunner::new("stub response");
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].step_id, "call_named");
    assert!(entries[0].response.is_some());
}

#[test]
fn named_pipeline_response_is_accessible_as_template_variable() {
    let named_steps = vec![prompt_step("inner", "do work")];
    let mut named_pipelines = HashMap::new();
    named_pipelines.insert("worker".to_string(), named_steps);

    let pipeline = Pipeline {
        steps: vec![
            make_named_pipeline_step("delegate", "worker"),
            prompt_step("followup", "Result was: {{ step.delegate.response }}"),
        ],
        source: None,
        defaults: Default::default(),
        timeout_seconds: None,
        default_tools: None,
        named_pipelines,
    };
    let mut session = Session::new(pipeline, "invocation prompt".to_string())
        .with_log_provider(Box::new(ail_core::session::log_provider::NullProvider));

    let runner = StubRunner::new("stub");
    execute(&mut session, &runner).unwrap();

    let entries = session.turn_log.entries();
    assert_eq!(entries.len(), 2);
    assert!(
        entries[1].prompt.starts_with("Result was:"),
        "Expected prompt with interpolated response, got: {}",
        entries[1].prompt
    );
}

// ── §10.3 Named pipeline with prompt override ───────────────────────────────

#[test]
fn named_pipeline_with_prompt_override_passes_prompt_to_child() {
    let path = fixtures_dir().join("named_pipeline_with_prompt.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    let mut session = Session::new(pipeline, "original prompt".to_string())
        .with_log_provider(Box::new(ail_core::session::log_provider::NullProvider));

    let runner = EchoStubRunner::new();
    let result = execute(&mut session, &runner);
    assert!(result.is_ok(), "Expected Ok, got: {result:?}");

    let last = session.turn_log.last_response().unwrap_or("");
    assert!(
        last.contains("Please review this code carefully"),
        "Expected response to contain the prompt override, got: {last:?}"
    );
}

// ── §10.4 Circular reference detection ──────────────────────────────────────

#[test]
fn circular_named_pipeline_reference_detected_at_load_time() {
    let path = fixtures_dir().join("named_pipeline_circular.ail.yaml");
    let result = ail_core::config::load(&path);
    assert!(
        result.is_err(),
        "Expected circular reference error at load time"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.error_type(),
        error_types::PIPELINE_CIRCULAR_REFERENCE,
        "Expected PIPELINE_CIRCULAR_REFERENCE, got: {}",
        err.error_type()
    );
    assert!(
        err.detail().contains("ircular"),
        "Error detail should mention circular reference: {}",
        err.detail()
    );
}

#[test]
fn self_referencing_named_pipeline_detected_at_load_time() {
    let tmp = tempfile::tempdir().unwrap();
    let yaml = r#"
version: "0.0.1"
pipelines:
  self_ref:
    - id: recurse
      pipeline: self_ref
pipeline:
  - id: start
    pipeline: self_ref
"#;
    let path = tmp.path().join("self_ref.ail.yaml");
    std::fs::write(&path, yaml).unwrap();
    let result = ail_core::config::load(&path);
    assert!(result.is_err(), "Expected circular reference error");
    assert_eq!(
        result.unwrap_err().error_type(),
        error_types::PIPELINE_CIRCULAR_REFERENCE
    );
}

// ── §10.5 Named pipeline not found ──────────────────────────────────────────

#[test]
fn undefined_named_pipeline_reference_returns_error() {
    let pipeline = Pipeline {
        steps: vec![make_named_pipeline_step("call", "nonexistent")],
        source: None,
        defaults: Default::default(),
        timeout_seconds: None,
        default_tools: None,
        named_pipelines: HashMap::new(),
    };
    let mut session = Session::new(pipeline, "trigger".to_string())
        .with_log_provider(Box::new(ail_core::session::log_provider::NullProvider));

    let runner = StubRunner::new("stub");
    let result = execute(&mut session, &runner);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_type(), error_types::PIPELINE_ABORTED);
    assert!(
        err.detail().contains("nonexistent"),
        "Error should mention the missing pipeline name: {}",
        err.detail()
    );
}

// ── §10.6 Validation: empty named pipeline ──────────────────────────────────

#[test]
fn empty_named_pipeline_is_validation_error() {
    let tmp = tempfile::tempdir().unwrap();
    let yaml = r#"
version: "0.0.1"
pipelines:
  empty_one: []
pipeline:
  - id: s1
    prompt: "hello"
"#;
    let path = tmp.path().join("empty_named.ail.yaml");
    std::fs::write(&path, yaml).unwrap();
    let result = ail_core::config::load(&path);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type(),
        error_types::CONFIG_VALIDATION_FAILED
    );
}

// ── §10.7 File-based pipeline: path not reclassified as named ───────────────

#[test]
fn file_path_pipeline_step_not_reclassified_when_named_pipelines_present() {
    let path = fixtures_dir().join("named_pipelines.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    // main_step is a prompt, not a pipeline reference
    let main = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "main_step")
        .unwrap();
    assert!(matches!(main.body, StepBody::Prompt(_)));
}

// ── §17 materialize --expand-pipelines ──────────────────────────────────────

#[test]
fn materialize_includes_named_pipelines_section() {
    let path = fixtures_dir().join("named_pipelines.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    let output = ail_core::materialize::materialize(&pipeline);
    assert!(
        output.contains("pipelines:"),
        "Expected pipelines: section in materialize output, got:\n{output}"
    );
    assert!(
        output.contains("security_gates:"),
        "Expected security_gates in output"
    );
    assert!(
        output.contains("quality_check:"),
        "Expected quality_check in output"
    );
}

#[test]
fn materialize_expanded_inlines_named_pipeline_steps() {
    let path = fixtures_dir().join("named_pipelines.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    let output =
        ail_core::materialize::materialize_expanded(&pipeline).expect("expand should succeed");
    // The expanded output should contain the inner step IDs from security_gates
    assert!(
        output.contains("vuln_scan"),
        "Expected vuln_scan step in expanded output, got:\n{output}"
    );
    assert!(
        output.contains("license_check"),
        "Expected license_check in expanded output"
    );
    assert!(
        output.contains("lint"),
        "Expected lint step in expanded output"
    );
    // Should have expansion comments
    assert!(
        output.contains("expanded from named pipeline"),
        "Expected expansion comment in output"
    );
    // Should NOT have a pipelines: section (only pipeline: with inlined steps)
    assert!(
        !output.contains("\npipelines:\n"),
        "Expanded output should not contain pipelines: section"
    );
}

#[test]
fn materialize_expanded_circular_returns_error() {
    // Build a circular pipeline manually (bypassing validation) to test
    // that materialize_expanded also catches cycles at expansion time.
    let named_steps_a = vec![make_named_pipeline_step("call_b", "beta")];
    let named_steps_b = vec![make_named_pipeline_step("call_a", "alpha")];
    let mut named_pipelines = HashMap::new();
    named_pipelines.insert("alpha".to_string(), named_steps_a);
    named_pipelines.insert("beta".to_string(), named_steps_b);

    let pipeline = Pipeline {
        steps: vec![make_named_pipeline_step("start", "alpha")],
        source: None,
        defaults: Default::default(),
        timeout_seconds: None,
        default_tools: None,
        named_pipelines,
    };
    let result = ail_core::materialize::materialize_expanded(&pipeline);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_type(),
        error_types::PIPELINE_CIRCULAR_REFERENCE
    );
}

#[test]
fn materialize_expanded_produces_valid_yaml() {
    let path = fixtures_dir().join("named_pipelines.ail.yaml");
    let pipeline = ail_core::config::load(&path).unwrap();
    let output =
        ail_core::materialize::materialize_expanded(&pipeline).expect("expand should succeed");
    let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(&output);
    assert!(result.is_ok(), "Output was not valid YAML: {output}");
}
