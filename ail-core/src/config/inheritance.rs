//! Pipeline inheritance (SPEC §7) — FROM resolution, hook operations, and onion-model ordering.
//!
//! When a pipeline declares `FROM: <path>`, ail loads the base pipeline, resolves
//! hooks (run_before, run_after, override, disable), and merges settings/defaults
//! using child-wins-on-conflict semantics.

#![allow(clippy::result_large_err)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::AilError;

use super::dto::{DefaultsDto, PipelineFileDto, StepDto};

// ── Path resolution ──────────────────────────────────────────────────────────

/// Resolve a FROM path relative to the directory containing the referencing file.
/// Supports relative paths, absolute paths, and home-relative (`~`) paths.
pub fn resolve_from_path(from_value: &str, referencing_file: &Path) -> Result<PathBuf, AilError> {
    let expanded = if let Some(stripped) = from_value.strip_prefix('~') {
        let home = dirs::home_dir().ok_or_else(|| {
            AilError::config_validation(format!(
                "FROM path '{from_value}' uses ~ but home directory could not be determined",
            ))
        })?;
        let suffix = stripped.strip_prefix('/').unwrap_or(stripped);
        home.join(suffix)
    } else {
        PathBuf::from(from_value)
    };

    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        // Relative to the directory containing the referencing file.
        let base_dir = referencing_file.parent().unwrap_or_else(|| Path::new("."));
        Ok(base_dir.join(expanded))
    }
}

/// Canonicalize a path for cycle detection. Falls back to the original path
/// if canonicalization fails (e.g. the file doesn't exist yet).
pub fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// ── Cycle detection ──────────────────────────────────────────────────────────

/// Check that adding `new_path` to the chain does not create a cycle.
/// `chain` contains the canonical paths of files already in the inheritance chain.
/// Returns `Err` with the full chain displayed if a cycle is detected.
pub fn check_cycle(chain: &[PathBuf], new_path: &Path) -> Result<(), AilError> {
    let new_canonical = canonical_path(new_path);
    if chain.contains(&new_canonical) {
        let mut display_chain: Vec<String> =
            chain.iter().map(|p| p.display().to_string()).collect();
        display_chain.push(new_canonical.display().to_string());
        let chain_str = display_chain.join(" \u{2192} ");
        return Err(AilError::circular_inheritance(format!(
            "circular inheritance detected\n  {chain_str}\n  \
             ail cannot resolve a pipeline that inherits from itself."
        )));
    }
    Ok(())
}

// ── DTO-level loading with FROM resolution ───────────────────────────────────

/// Parse a YAML file into a DTO, used internally by the FROM resolver.
fn parse_dto(path: &Path) -> Result<PipelineFileDto, AilError> {
    let contents = std::fs::read_to_string(path).map_err(|e| AilError::ConfigFileNotFound {
        detail: format!("Could not read '{}': {e}", path.display()),
        context: None,
    })?;
    serde_yaml::from_str(&contents).map_err(|e| AilError::ConfigInvalidYaml {
        detail: format!("Failed to parse '{}': {e}", path.display()),
        context: None,
    })
}

/// Recursively load a pipeline DTO chain, resolving `FROM` references.
/// Returns the fully merged DTO (base first, child overrides on top).
///
/// `chain` tracks canonical paths for cycle detection.
pub fn load_with_inheritance(
    path: &Path,
    chain: &mut Vec<PathBuf>,
) -> Result<PipelineFileDto, AilError> {
    let canonical = canonical_path(path);
    check_cycle(chain, path)?;
    chain.push(canonical);

    let child_dto = parse_dto(path)?;

    let base_dto = match &child_dto.from {
        Some(from_value) => {
            let base_path = resolve_from_path(from_value, path)?;
            Some(load_with_inheritance(&base_path, chain)?)
        }
        None => None,
    };

    match base_dto {
        Some(base) => merge_dtos(base, child_dto, path),
        None => Ok(child_dto),
    }
}

// ── DTO merging ──────────────────────────────────────────────────────────────

/// Merge a base DTO and a child DTO, applying hook operations.
///
/// Child settings/defaults override base settings. Steps use the hook-based
/// merging algorithm from SPEC §7.2 and the onion model from SPEC §8.
fn merge_dtos(
    base: PipelineFileDto,
    child: PipelineFileDto,
    child_path: &Path,
) -> Result<PipelineFileDto, AilError> {
    // Version: child wins if present, otherwise base.
    let version = child.version.or(base.version);

    // Defaults: merge child on top of base (child wins on conflict).
    let defaults = merge_defaults(base.defaults, child.defaults);

    // Steps: apply hook operations from child to base steps.
    let base_steps = base.pipeline.unwrap_or_default();
    let child_steps = child.pipeline.unwrap_or_default();

    let merged_steps = apply_hooks(base_steps, child_steps, child_path)?;

    Ok(PipelineFileDto {
        version,
        from: None, // FROM is consumed during resolution
        defaults,
        pipeline: if merged_steps.is_empty() {
            None
        } else {
            Some(merged_steps)
        },
        pipelines: None, // named pipelines are not inherited (SPEC §10)
    })
}

/// Merge defaults DTOs. Child values override base values.
fn merge_defaults(base: Option<DefaultsDto>, child: Option<DefaultsDto>) -> Option<DefaultsDto> {
    match (base, child) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(c)) => Some(c),
        (Some(b), Some(c)) => Some(DefaultsDto {
            model: c.model.or(b.model),
            provider: match (b.provider, c.provider) {
                (None, None) => None,
                (Some(bp), None) => Some(bp),
                (None, Some(cp)) => Some(cp),
                (Some(bp), Some(cp)) => Some(super::dto::ProviderDto {
                    model: cp.model.or(bp.model),
                    base_url: cp.base_url.or(bp.base_url),
                    auth_token: cp.auth_token.or(bp.auth_token),
                    connect_timeout_seconds: cp
                        .connect_timeout_seconds
                        .or(bp.connect_timeout_seconds),
                    read_timeout_seconds: cp.read_timeout_seconds.or(bp.read_timeout_seconds),
                    max_history_messages: cp.max_history_messages.or(bp.max_history_messages),
                }),
            },
            timeout_seconds: c.timeout_seconds.or(b.timeout_seconds),
            tools: c.tools.or(b.tools),
            max_concurrency: c.max_concurrency.or(b.max_concurrency),
        }),
    }
}

/// Apply hook operations from child steps to base steps.
///
/// Algorithm:
/// 1. Classify each child step as a hook op or a plain step.
/// 2. Validate all hook targets exist in the base step list.
/// 3. Validate no duplicate (target, operation) pairs in the same file.
/// 4. Apply disable operations (remove steps).
/// 5. Apply override operations (replace steps in place).
/// 6. Apply run_before and run_after operations (insert steps).
/// 7. Append plain steps at the end.
fn apply_hooks(
    base_steps: Vec<StepDto>,
    child_steps: Vec<StepDto>,
    child_path: &Path,
) -> Result<Vec<StepDto>, AilError> {
    // Classify child steps into categories.
    let mut disables: Vec<String> = Vec::new();
    let mut overrides: Vec<(String, StepDto)> = Vec::new();
    let mut before_hooks: Vec<(String, StepDto)> = Vec::new();
    let mut after_hooks: Vec<(String, StepDto)> = Vec::new();
    let mut plain_steps: Vec<StepDto> = Vec::new();

    for step in child_steps {
        let hook_count = [
            step.run_before.is_some(),
            step.run_after.is_some(),
            step.override_step.is_some(),
            step.disable.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if hook_count > 1 {
            let id_hint = step.id.as_deref().unwrap_or("<unnamed>");
            return Err(AilError::config_validation(format!(
                "Step '{id_hint}' declares multiple hook operations \
                 (run_before, run_after, override, disable); at most one is allowed"
            )));
        }

        if let Some(ref target) = step.disable {
            disables.push(target.clone());
        } else if let Some(ref target) = step.override_step {
            overrides.push((target.clone(), step));
        } else if let Some(ref target) = step.run_before {
            before_hooks.push((target.clone(), step));
        } else if let Some(ref target) = step.run_after {
            after_hooks.push((target.clone(), step));
        } else {
            plain_steps.push(step);
        }
    }

    // Collect base step IDs for target validation.
    let base_ids: HashSet<String> = base_steps.iter().filter_map(|s| s.id.clone()).collect();

    // Validate all hook targets exist in the base.
    let mut seen_hooks: HashSet<(String, String)> = HashSet::new();

    for target in &disables {
        validate_hook_target(target, "disable", &base_ids, child_path)?;
        check_duplicate_hook(target, "disable", &mut seen_hooks, child_path)?;
    }
    for (target, step) in &overrides {
        validate_hook_target(target, "override", &base_ids, child_path)?;
        check_duplicate_hook(target, "override", &mut seen_hooks, child_path)?;
        // Validate override id constraint.
        if let Some(ref id) = step.id {
            if id != target {
                return Err(AilError::config_validation(format!(
                    "override: {target} declares id '{id}' which differs from the \
                     target step ID -- override must not change the step ID"
                )));
            }
        }
    }
    for (target, _) in &before_hooks {
        validate_hook_target(target, "run_before", &base_ids, child_path)?;
        check_duplicate_hook(target, "run_before", &mut seen_hooks, child_path)?;
    }
    for (target, _) in &after_hooks {
        validate_hook_target(target, "run_after", &base_ids, child_path)?;
        check_duplicate_hook(target, "run_after", &mut seen_hooks, child_path)?;
    }

    // Start with base steps.
    let mut result: Vec<StepDto> = base_steps;

    // 1. Apply disable operations — remove steps.
    let disable_set: HashSet<&String> = disables.iter().collect();
    result.retain(|s| {
        s.id.as_ref()
            .map(|id| !disable_set.contains(id))
            .unwrap_or(true)
    });

    // 2. Apply override operations — replace step bodies in place.
    for (target, mut step) in overrides {
        if let Some(pos) = result
            .iter()
            .position(|s| s.id.as_deref() == Some(target.as_str()))
        {
            // The override step inherits the target's ID if not explicitly set.
            if step.id.is_none() {
                step.id = Some(target);
            }
            // Clear the hook operation field from the merged step.
            step.override_step = None;
            result[pos] = step;
        }
    }

    // 3. Apply run_before and run_after operations (onion model: child is outermost).
    //    Group by target for efficient insertion.
    let mut before_map: HashMap<String, Vec<StepDto>> = HashMap::new();
    let mut after_map: HashMap<String, Vec<StepDto>> = HashMap::new();

    for (target, mut step) in before_hooks {
        step.run_before = None;
        before_map.entry(target).or_default().push(step);
    }
    for (target, mut step) in after_hooks {
        step.run_after = None;
        after_map.entry(target).or_default().push(step);
    }

    if !before_map.is_empty() || !after_map.is_empty() {
        let mut expanded: Vec<StepDto> = Vec::with_capacity(result.len() * 2);
        for step in result {
            let step_id = step.id.clone().unwrap_or_default();
            // Insert run_before hooks before the target step.
            if let Some(steps) = before_map.remove(&step_id) {
                expanded.extend(steps);
            }
            expanded.push(step);
            // Insert run_after hooks after the target step.
            if let Some(steps) = after_map.remove(&step_id) {
                expanded.extend(steps);
            }
        }
        result = expanded;
    }

    // 4. Append plain (non-hook) child steps at the end.
    result.extend(plain_steps);

    Ok(result)
}

fn validate_hook_target(
    target: &str,
    op_name: &str,
    base_ids: &HashSet<String>,
    child_path: &Path,
) -> Result<(), AilError> {
    if !base_ids.contains(target) {
        let mut valid_ids: Vec<&String> = base_ids.iter().collect();
        valid_ids.sort();
        return Err(AilError::config_validation(format!(
            "Hook operation '{op_name}: {target}' in '{}' targets a step ID that does not \
             exist in the inheritance chain. Valid step IDs: {valid_ids:?}",
            child_path.display(),
        )));
    }
    Ok(())
}

fn check_duplicate_hook(
    target: &str,
    op_name: &str,
    seen: &mut HashSet<(String, String)>,
    child_path: &Path,
) -> Result<(), AilError> {
    let key = (target.to_string(), op_name.to_string());
    if !seen.insert(key) {
        return Err(AilError::config_validation(format!(
            "Duplicate hook operation '{op_name}: {target}' in '{}' -- \
             use sequential steps instead",
            child_path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_yaml(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    // ── resolve_from_path ────────────────────────────────────────────────────

    #[test]
    fn resolve_relative_from_path() {
        let result = resolve_from_path("./base.yaml", Path::new("/project/.ail.yaml")).unwrap();
        assert_eq!(result, PathBuf::from("/project/base.yaml"));
    }

    #[test]
    fn resolve_absolute_from_path() {
        let result =
            resolve_from_path("/etc/ail/base.yaml", Path::new("/project/.ail.yaml")).unwrap();
        assert_eq!(result, PathBuf::from("/etc/ail/base.yaml"));
    }

    // ── cycle detection ──────────────────────────────────────────────────────

    #[test]
    fn check_cycle_no_cycle() {
        let chain = vec![PathBuf::from("/a.yaml"), PathBuf::from("/b.yaml")];
        assert!(check_cycle(&chain, Path::new("/c.yaml")).is_ok());
    }

    #[test]
    fn check_cycle_detects_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_yaml(&dir, "a.yaml", "");
        let chain = vec![canonical_path(&a)];
        let err = check_cycle(&chain, &a).unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CIRCULAR_INHERITANCE
        );
        assert!(err.detail().contains("circular inheritance detected"));
    }

    // ── load_with_inheritance ────────────────────────────────────────────────

    #[test]
    fn load_simple_no_from() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\npipeline:\n  - id: step1\n    prompt: hello\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&path, &mut chain).unwrap();
        assert_eq!(dto.pipeline.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn load_single_from() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: base_step\n    prompt: base\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - id: child_step\n    prompt: child\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id.as_deref(), Some("base_step"));
        assert_eq!(steps[1].id.as_deref(), Some("child_step"));
    }

    #[test]
    fn load_circular_from_detected() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = dir.path().join("a.yaml");
        let b_path = dir.path().join("b.yaml");

        std::fs::write(
            &a_path,
            "version: \"1\"\nFROM: ./b.yaml\npipeline:\n  - id: a_step\n    prompt: a\n",
        )
        .unwrap();
        std::fs::write(
            &b_path,
            "version: \"1\"\nFROM: ./a.yaml\npipeline:\n  - id: b_step\n    prompt: b\n",
        )
        .unwrap();

        let mut chain = Vec::new();
        let err = load_with_inheritance(&a_path, &mut chain).unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CIRCULAR_INHERITANCE
        );
    }

    #[test]
    fn override_replaces_step() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: review\n    prompt: old review\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - override: review\n    prompt: new review\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id.as_deref(), Some("review"));
        assert_eq!(steps[0].prompt.as_deref(), Some("new review"));
    }

    #[test]
    fn disable_removes_step() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: keep_me\n    prompt: keep\n  - id: remove_me\n    prompt: remove\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - disable: remove_me\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id.as_deref(), Some("keep_me"));
    }

    #[test]
    fn run_before_inserts_step() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: target\n    prompt: target\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - run_before: target\n    id: before_step\n    prompt: before\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id.as_deref(), Some("before_step"));
        assert_eq!(steps[1].id.as_deref(), Some("target"));
    }

    #[test]
    fn run_after_inserts_step() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: target\n    prompt: target\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - run_after: target\n    id: after_step\n    prompt: after\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id.as_deref(), Some("target"));
        assert_eq!(steps[1].id.as_deref(), Some("after_step"));
    }

    #[test]
    fn hook_targeting_nonexistent_step_fails() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: real_step\n    prompt: real\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - disable: nonexistent\n",
        );
        let mut chain = Vec::new();
        let err = load_with_inheritance(&child_path, &mut chain).unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CONFIG_VALIDATION_FAILED
        );
        assert!(err.detail().contains("nonexistent"));
    }

    #[test]
    fn override_with_different_id_fails() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: target\n    prompt: old\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - override: target\n    id: different_id\n    prompt: new\n",
        );
        let mut chain = Vec::new();
        let err = load_with_inheritance(&child_path, &mut chain).unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CONFIG_VALIDATION_FAILED
        );
        assert!(err.detail().contains("different_id"));
    }

    #[test]
    fn duplicate_hook_same_target_fails() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\npipeline:\n  - id: target\n    prompt: t\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\npipeline:\n  - run_before: target\n    id: h1\n    prompt: h1\n  - run_before: target\n    id: h2\n    prompt: h2\n",
        );
        let mut chain = Vec::new();
        let err = load_with_inheritance(&child_path, &mut chain).unwrap_err();
        assert_eq!(
            err.error_type(),
            crate::error::error_types::CONFIG_VALIDATION_FAILED
        );
        assert!(err.detail().contains("Duplicate"));
    }

    #[test]
    fn child_defaults_override_base_defaults() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "base.yaml",
            "version: \"1\"\ndefaults:\n  model: base-model\n  timeout_seconds: 60\npipeline:\n  - id: s\n    prompt: s\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./base.yaml\ndefaults:\n  model: child-model\npipeline: []\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let defaults = dto.defaults.unwrap();
        assert_eq!(defaults.model.as_deref(), Some("child-model"));
        assert_eq!(defaults.timeout_seconds, Some(60));
    }

    #[test]
    fn three_level_inheritance() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "grandparent.yaml",
            "version: \"1\"\npipeline:\n  - id: gp_step\n    prompt: gp\n",
        );
        write_yaml(
            &dir,
            "parent.yaml",
            "version: \"1\"\nFROM: ./grandparent.yaml\npipeline:\n  - id: parent_step\n    prompt: parent\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./parent.yaml\npipeline:\n  - id: child_step\n    prompt: child\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].id.as_deref(), Some("gp_step"));
        assert_eq!(steps[1].id.as_deref(), Some("parent_step"));
        assert_eq!(steps[2].id.as_deref(), Some("child_step"));
    }

    #[test]
    fn grandchild_can_hook_grandparent_step() {
        let dir = tempfile::tempdir().unwrap();
        write_yaml(
            &dir,
            "grandparent.yaml",
            "version: \"1\"\npipeline:\n  - id: gp_step\n    prompt: gp\n",
        );
        write_yaml(
            &dir,
            "parent.yaml",
            "version: \"1\"\nFROM: ./grandparent.yaml\npipeline:\n  - id: parent_step\n    prompt: parent\n",
        );
        let child_path = write_yaml(
            &dir,
            "child.yaml",
            "version: \"1\"\nFROM: ./parent.yaml\npipeline:\n  - override: gp_step\n    prompt: overridden gp\n",
        );
        let mut chain = Vec::new();
        let dto = load_with_inheritance(&child_path, &mut chain).unwrap();
        let steps = dto.pipeline.unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id.as_deref(), Some("gp_step"));
        assert_eq!(steps[0].prompt.as_deref(), Some("overridden gp"));
    }
}
