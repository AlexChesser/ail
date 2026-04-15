//! SPEC §30 — Sampling Parameter Control.
//!
//! Covers the three-scope merge chain (pipeline defaults → provider-attached →
//! per-step), parse-time validation, thinking normalization (f64 + bool),
//! executor flow into `InvokeOptions`, and turn-log recording.

use std::path::PathBuf;

use ail_core::config::domain::{Pipeline, SamplingConfig, Step, StepBody, StepId};
use ail_core::config::load;
use ail_core::test_helpers::prompt_step;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ── Parse-time ──────────────────────────────────────────────────────────────

#[test]
fn sampling_defaults_parse_from_pipeline_file() {
    let pipeline = load(&fixtures_dir().join("sampling_basic.ail.yaml")).expect("should load");
    let defaults = pipeline
        .sampling_defaults
        .as_ref()
        .expect("sampling_defaults should be set");
    assert_eq!(defaults.temperature, Some(0.3));
    assert_eq!(defaults.max_tokens, Some(4096));
    assert_eq!(defaults.top_p, None);
}

#[test]
fn per_step_sampling_parses() {
    let pipeline = load(&fixtures_dir().join("sampling_basic.ail.yaml")).expect("should load");

    let inherit = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "inherit_defaults")
        .unwrap();
    assert!(inherit.sampling.is_none(), "inherits pipeline defaults");

    let override_step = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "override_temperature")
        .unwrap();
    let s = override_step.sampling.as_ref().unwrap();
    assert_eq!(s.temperature, Some(0.9));
    assert_eq!(s.max_tokens, None, "only temperature overridden at step");

    let full = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "full_override")
        .unwrap();
    let s = full.sampling.as_ref().unwrap();
    assert_eq!(s.temperature, Some(0.0));
    assert_eq!(s.top_p, Some(0.95));
    assert_eq!(s.top_k, Some(40));
    assert_eq!(s.max_tokens, Some(8192));
    assert_eq!(
        s.stop_sequences.as_deref(),
        Some(&["Human:".to_string(), "</answer>".to_string()][..])
    );
    assert_eq!(s.thinking, Some(0.75));
}

#[test]
fn provider_attached_sampling_parses() {
    let pipeline =
        load(&fixtures_dir().join("sampling_provider_attached.ail.yaml")).expect("should load");
    let provider_sampling = pipeline
        .defaults
        .sampling
        .as_ref()
        .expect("provider.sampling should be set");
    assert_eq!(provider_sampling.temperature, Some(0.2));
    assert_eq!(provider_sampling.thinking, Some(0.75));
    // pipeline-wide sampling_defaults are absent — only provider-attached is set
    assert!(pipeline.sampling_defaults.is_none());
}

#[test]
fn thinking_accepts_bool_and_float() {
    let pipeline =
        load(&fixtures_dir().join("sampling_thinking_bool.ail.yaml")).expect("should load");

    let on = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "thinking_on")
        .unwrap();
    assert_eq!(on.sampling.as_ref().unwrap().thinking, Some(1.0));

    let off = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "thinking_off")
        .unwrap();
    assert_eq!(off.sampling.as_ref().unwrap().thinking, Some(0.0));

    let frac = pipeline
        .steps
        .iter()
        .find(|s| s.id.as_str() == "thinking_fraction")
        .unwrap();
    assert_eq!(frac.sampling.as_ref().unwrap().thinking, Some(0.5));
}

#[test]
fn temperature_out_of_range_is_rejected() {
    let err = load(&fixtures_dir().join("invalid_sampling_temperature.ail.yaml"))
        .expect_err("out-of-range temperature must be rejected");
    assert!(
        format!("{err:?}").contains("temperature"),
        "error should mention the offending field: {err:?}"
    );
}

#[test]
fn empty_stop_sequences_is_rejected() {
    let err = load(&fixtures_dir().join("invalid_sampling_stop_sequences.ail.yaml"))
        .expect_err("empty stop_sequences must be rejected");
    assert!(
        format!("{err:?}").contains("stop_sequences"),
        "error should mention the offending field: {err:?}"
    );
}

// ── Merge semantics ─────────────────────────────────────────────────────────

#[test]
fn sampling_merge_higher_wins_per_field() {
    let base = SamplingConfig {
        temperature: Some(0.3),
        max_tokens: Some(4096),
        top_p: Some(0.9),
        ..Default::default()
    };
    let override_ = SamplingConfig {
        temperature: Some(0.9),
        thinking: Some(0.75),
        ..Default::default()
    };
    let merged = base.merge(override_);
    assert_eq!(
        merged.temperature,
        Some(0.9),
        "override wins on temperature"
    );
    assert_eq!(merged.max_tokens, Some(4096), "base falls through");
    assert_eq!(merged.top_p, Some(0.9), "base falls through");
    assert_eq!(
        merged.thinking,
        Some(0.75),
        "new field from override is kept"
    );
}

#[test]
fn sampling_merge_stop_sequences_replaces_not_appends() {
    // SPEC §30.3.1 — stop_sequences replaces, does not append.
    let base = SamplingConfig {
        stop_sequences: Some(vec!["Human:".to_string()]),
        ..Default::default()
    };
    let override_ = SamplingConfig {
        stop_sequences: Some(vec!["</answer>".to_string()]),
        ..Default::default()
    };
    let merged = base.merge(override_);
    assert_eq!(
        merged.stop_sequences.as_deref(),
        Some(&["</answer>".to_string()][..]),
        "step-level list replaces inherited; no append"
    );
}

#[test]
fn sampling_is_empty_reports_correctly() {
    assert!(SamplingConfig::default().is_empty());
    let cfg = SamplingConfig {
        temperature: Some(0.5),
        ..Default::default()
    };
    assert!(!cfg.is_empty());
}

// ── Executor flow ───────────────────────────────────────────────────────────

#[test]
fn resolved_sampling_reaches_runner_via_invoke_options() {
    use ail_core::runner::{InvokeOptions, RunResult, Runner};
    use std::sync::Arc;
    use std::sync::Mutex;

    // Inline spy runner that captures InvokeOptions.sampling.
    #[derive(Default)]
    struct Spy {
        seen: Arc<Mutex<Vec<Option<SamplingConfig>>>>,
    }
    impl Runner for Spy {
        fn invoke(
            &self,
            _prompt: &str,
            options: InvokeOptions,
        ) -> Result<RunResult, ail_core::error::AilError> {
            self.seen.lock().unwrap().push(options.sampling);
            Ok(RunResult {
                response: "ok".to_string(),
                cost_usd: None,
                session_id: None,
                input_tokens: 0,
                output_tokens: 0,
                thinking: None,
                model: None,
                tool_events: vec![],
            })
        }
    }

    let step = Step {
        id: StepId("with_sampling".to_string()),
        body: StepBody::Prompt("hi".to_string()),
        sampling: Some(SamplingConfig {
            temperature: Some(0.4),
            max_tokens: Some(1024),
            thinking: Some(0.25),
            ..Default::default()
        }),
        ..Default::default()
    };

    let pipeline = Pipeline {
        steps: vec![step],
        sampling_defaults: Some(SamplingConfig {
            top_p: Some(0.95),
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut session = ail_core::session::Session::new(pipeline, "prompt".to_string())
        .with_log_provider(Box::new(ail_core::session::log_provider::NullProvider));

    let spy = Spy::default();
    let seen = spy.seen.clone();
    ail_core::executor::execute(&mut session, &spy).expect("pipeline runs");

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "spy should see one invocation");
    let s = seen[0].as_ref().expect("sampling should reach the runner");
    assert_eq!(s.temperature, Some(0.4), "step wins on temperature");
    assert_eq!(s.max_tokens, Some(1024), "step carries max_tokens");
    assert_eq!(s.thinking, Some(0.25));
    assert_eq!(
        s.top_p,
        Some(0.95),
        "pipeline default top_p falls through to the runner"
    );
}

#[test]
fn no_sampling_anywhere_yields_none_at_runner() {
    use ail_core::runner::{InvokeOptions, RunResult, Runner};
    use std::sync::Arc;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Spy {
        seen: Arc<Mutex<Vec<Option<SamplingConfig>>>>,
    }
    impl Runner for Spy {
        fn invoke(
            &self,
            _prompt: &str,
            options: InvokeOptions,
        ) -> Result<RunResult, ail_core::error::AilError> {
            self.seen.lock().unwrap().push(options.sampling);
            Ok(RunResult {
                response: "ok".to_string(),
                cost_usd: None,
                session_id: None,
                input_tokens: 0,
                output_tokens: 0,
                thinking: None,
                model: None,
                tool_events: vec![],
            })
        }
    }

    let pipeline = Pipeline {
        steps: vec![prompt_step("plain", "hi")],
        ..Default::default()
    };
    let mut session = ail_core::session::Session::new(pipeline, "prompt".to_string())
        .with_log_provider(Box::new(ail_core::session::log_provider::NullProvider));

    let spy = Spy::default();
    let seen = spy.seen.clone();
    ail_core::executor::execute(&mut session, &spy).expect("runs");
    assert!(
        seen.lock().unwrap()[0].is_none(),
        "no sampling at any scope → runner sees None"
    );
}
