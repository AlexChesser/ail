//! Runner selection and construction — resolving per-step runner overrides.

#![allow(clippy::result_large_err)]

use crate::config::domain::{ProviderConfig, SamplingConfig, Step};
use crate::error::AilError;
use crate::runner::factory::RunnerFactory;
use crate::runner::http::HttpSessionStore;
use crate::runner::Runner;
use crate::session::Session;

/// Resolve the effective provider config for a step by merging pipeline defaults,
/// step-level model override, and CLI provider flags.
pub(in crate::executor) fn resolve_step_provider(session: &Session, step: &Step) -> ProviderConfig {
    session
        .pipeline
        .defaults
        .clone()
        .merge(ProviderConfig {
            model: step.model.clone(),
            ..Default::default()
        })
        .merge(session.cli_provider.clone())
}

/// Resolve the effective sampling configuration for a step (SPEC §30.3).
///
/// Merges three scopes in low-to-high precedence order:
/// 1. `session.pipeline.sampling_defaults` — pipeline-wide baseline
///    (`defaults.sampling:` in YAML).
/// 2. `session.pipeline.defaults.sampling` — provider-attached
///    (`defaults.provider.sampling:` in YAML). When §15 named-provider
///    aliases land, this step will resolve against the step's chosen
///    provider profile instead of the single `defaults.provider` block.
/// 3. `step.sampling` — per-step override.
///
/// Returns `None` when the merged config is empty (nothing set at any
/// scope) so runners can cheaply skip the sampling path.
pub(in crate::executor) fn resolve_step_sampling(
    session: &Session,
    step: &Step,
) -> Option<SamplingConfig> {
    let pipeline_default = session
        .pipeline
        .sampling_defaults
        .clone()
        .unwrap_or_default();
    let provider_attached = session
        .pipeline
        .defaults
        .sampling
        .clone()
        .unwrap_or_default();
    let step_override = step.sampling.clone().unwrap_or_default();

    let merged = pipeline_default
        .merge(provider_attached)
        .merge(step_override);

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

/// Build a per-step runner override box if `step.runner` is set (SPEC §19).
///
/// `headless` is propagated from `Session.headless` so per-step `runner: claude` overrides
/// honour the same `--dangerously-skip-permissions` flag as the default runner.
pub(in crate::executor) fn build_step_runner_box(
    step: &Step,
    headless: bool,
    http_store: &HttpSessionStore,
    provider: &ProviderConfig,
) -> Result<Option<Box<dyn Runner + Send>>, AilError> {
    match step.runner {
        Some(ref name) => Ok(Some(RunnerFactory::build(
            name, headless, http_store, provider,
        )?)),
        None => Ok(None),
    }
}

/// Resolve the effective runner name for a step without constructing the runner.
///
/// Mirrors the `RunnerFactory` selection hierarchy:
/// 1. Per-step `runner:` field
/// 2. `AIL_DEFAULT_RUNNER` environment variable
/// 3. `"claude"` fallback
///
/// Used to update `session.runner_name` so `{{ session.tool }}` reflects the actual runner.
pub(in crate::executor) fn resolve_effective_runner_name(step: &Step) -> String {
    if let Some(ref name) = step.runner {
        name.clone()
    } else {
        std::env::var("AIL_DEFAULT_RUNNER").unwrap_or_else(|_| "claude".to_string())
    }
}
