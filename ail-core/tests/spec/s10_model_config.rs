/// SPEC §10 / §15 — provider/model config: pipeline defaults, per-step overrides, CLI override.
///
/// Precedence chain (low → high): pipeline defaults → per-step model → cli_provider.
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn provider_defaults_yaml_parses_model_and_provider() {
    let pipeline = ail_core::config::load(&fixtures_dir().join("provider_defaults.ail.yaml"))
        .expect("fixture should load");
    assert_eq!(pipeline.defaults.model.as_deref(), Some("gemma3:1b"));
    assert_eq!(
        pipeline.defaults.base_url.as_deref(),
        Some("http://localhost:11434")
    );
    assert_eq!(pipeline.defaults.auth_token.as_deref(), Some("ollama"));
}

#[test]
fn per_step_model_overrides_pipeline_default() {
    let pipeline = ail_core::config::load(&fixtures_dir().join("provider_defaults.ail.yaml"))
        .expect("fixture should load");
    let step_one = pipeline.steps.iter().find(|s| s.id.as_str() == "step_one");
    let step_two = pipeline.steps.iter().find(|s| s.id.as_str() == "step_two");
    assert!(step_one.is_some());
    assert!(step_two.is_some());
    // step_one inherits defaults; step_two overrides model
    assert_eq!(step_one.unwrap().model, None);
    assert_eq!(
        step_two.unwrap().model.as_deref(),
        Some("claude-haiku-4-5-20251001")
    );
}

#[test]
fn provider_config_merge_higher_wins() {
    use ail_core::config::domain::ProviderConfig;
    let base = ProviderConfig {
        model: Some("base-model".to_string()),
        base_url: Some("http://base".to_string()),
        auth_token: Some("base-token".to_string()),
        input_cost_per_1k: Some(0.0),
        output_cost_per_1k: Some(0.0),
    };
    let override_ = ProviderConfig {
        model: Some("override-model".to_string()),
        base_url: None,
        auth_token: None,
        input_cost_per_1k: None,
        output_cost_per_1k: None,
    };
    let merged = base.merge(override_);
    assert_eq!(merged.model.as_deref(), Some("override-model"));
    // base values fall through when override has None
    assert_eq!(merged.base_url.as_deref(), Some("http://base"));
    assert_eq!(merged.auth_token.as_deref(), Some("base-token"));
    assert_eq!(merged.input_cost_per_1k, Some(0.0));
    assert_eq!(merged.output_cost_per_1k, Some(0.0));
}

#[test]
fn provider_config_merge_all_none_returns_base() {
    use ail_core::config::domain::ProviderConfig;
    let base = ProviderConfig {
        model: Some("model".to_string()),
        base_url: Some("http://url".to_string()),
        auth_token: Some("token".to_string()),
        input_cost_per_1k: Some(0.001),
        output_cost_per_1k: Some(0.002),
    };
    let merged = base.clone().merge(ProviderConfig::default());
    assert_eq!(merged.model, base.model);
    assert_eq!(merged.base_url, base.base_url);
    assert_eq!(merged.auth_token, base.auth_token);
    assert_eq!(merged.input_cost_per_1k, base.input_cost_per_1k);
    assert_eq!(merged.output_cost_per_1k, base.output_cost_per_1k);
}

#[test]
fn pipeline_without_defaults_has_empty_provider_config() {
    let pipeline =
        ail_core::config::load(&fixtures_dir().join("minimal.ail.yaml")).expect("should load");
    assert!(pipeline.defaults.model.is_none());
    assert!(pipeline.defaults.base_url.is_none());
    assert!(pipeline.defaults.auth_token.is_none());
    assert!(pipeline.defaults.input_cost_per_1k.is_none());
    assert!(pipeline.defaults.output_cost_per_1k.is_none());
}

#[test]
fn provider_costs_yaml_parses_cost_fields() {
    let pipeline = ail_core::config::load(&fixtures_dir().join("provider_costs.ail.yaml"))
        .expect("fixture should load");
    assert_eq!(pipeline.defaults.model.as_deref(), Some("ollama"));
    assert_eq!(
        pipeline.defaults.base_url.as_deref(),
        Some("http://localhost:11434")
    );
    assert_eq!(pipeline.defaults.auth_token.as_deref(), Some("ollama"));
    assert_eq!(pipeline.defaults.input_cost_per_1k, Some(0.0));
    assert_eq!(pipeline.defaults.output_cost_per_1k, Some(0.0));
}

#[test]
fn invoke_options_carries_resolved_model() {
    use ail_core::config::domain::{Pipeline, ProviderConfig, Step, StepBody, StepId};
    use ail_core::runner::stub::CountingStubRunner;
    use ail_core::session::Session;

    // Pipeline defaults: model = "default-model"
    let step = Step {
        id: StepId("s".to_string()),
        body: StepBody::Prompt("hello".to_string()),
        message: None,
        tools: None,
        on_result: None,
        model: None,
        runner: None,
        condition: None,
        append_system_prompt: None,
        system_prompt: None,
        resume: false,
    };
    let pipeline = Pipeline {
        steps: vec![step],
        source: None,
        defaults: ProviderConfig {
            model: Some("default-model".to_string()),
            base_url: None,
            auth_token: None,
            input_cost_per_1k: None,
            output_cost_per_1k: None,
        },
        timeout_seconds: None,
        default_tools: None,
    };
    let mut session = Session::new(pipeline, "prompt".to_string());
    let runner = CountingStubRunner::new("ok");
    ail_core::executor::execute(&mut session, &runner).unwrap();
    // The runner was called — this confirms execute() ran without error.
    // The model field is wired through InvokeOptions; full round-trip requires
    // an integration test with ClaudeCliRunner (marked #[ignore]).
    assert_eq!(runner.invocation_count(), 1);
}
