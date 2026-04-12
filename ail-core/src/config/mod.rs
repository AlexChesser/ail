#![allow(clippy::result_large_err)]

pub mod discovery;
pub mod domain;
pub use discovery::PipelineEntry;
pub mod dto;
pub mod inheritance;
pub mod validation;

use std::path::Path;

use crate::error::AilError;
use domain::Pipeline;
use validation::validate;

pub fn load(path: &Path) -> Result<Pipeline, AilError> {
    // Normalize to absolute so parent() always returns a usable directory (SPEC §9).
    // Guards against bare filenames (e.g. `.ail.yaml`) where parent() returns "" instead
    // of None — an empty base joined with "./relative" leaves the path unresolved.
    let abs_source = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    // Load the pipeline DTO with FROM inheritance resolution (SPEC §7).
    let mut chain = Vec::new();
    let dto = inheritance::load_with_inheritance(&abs_source, &mut chain)?;

    validate(dto, abs_source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::error_types;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        // CARGO_MANIFEST_DIR resolves to the ail-core crate root at test time.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    /// load() returns Ok(Pipeline) for a well-formed fixture file.
    #[test]
    fn load_valid_fixture_returns_pipeline() {
        let result = load(&fixtures_dir().join("minimal.ail.yaml"));
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    /// The returned pipeline has the expected step count and id.
    #[test]
    fn load_minimal_fixture_has_correct_step() {
        let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
        assert_eq!(pipeline.steps.len(), 1);
        assert_eq!(pipeline.steps[0].id.as_str(), "dont_be_stupid");
    }

    /// Loading a fixture with multiple steps returns all of them.
    #[test]
    fn load_multi_step_fixture_returns_all_steps() {
        let pipeline = load(&fixtures_dir().join("solo_developer.ail.yaml")).unwrap();
        assert_eq!(pipeline.steps.len(), 2);
        let ids: Vec<&str> = pipeline.steps.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"dry_refactor"), "dry_refactor not found");
        assert!(ids.contains(&"security_audit"), "security_audit not found");
    }

    /// The source field on the returned Pipeline matches the path that was loaded.
    #[test]
    fn load_sets_source_to_absolute_path() {
        let path = fixtures_dir().join("minimal.ail.yaml");
        let pipeline = load(&path).unwrap();
        let source = pipeline.source.expect("source must be set after load");
        assert!(source.is_absolute(), "source must be an absolute path");
        assert!(
            source.to_string_lossy().contains("minimal.ail.yaml"),
            "source path should reference the loaded file"
        );
    }

    /// Loading a nonexistent file returns CONFIG_FILE_NOT_FOUND.
    #[test]
    fn load_missing_file_returns_file_not_found_error() {
        let path = PathBuf::from("/no/such/path/pipeline.ail.yaml");
        let err = load(&path).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::CONFIG_FILE_NOT_FOUND,
            "wrong error_type: got {}",
            err.error_type()
        );
    }

    /// The error detail for a missing file contains the path.
    #[test]
    fn load_missing_file_error_mentions_path() {
        let path = PathBuf::from("/no/such/path/pipeline.ail.yaml");
        let err = load(&path).unwrap_err();
        assert!(
            err.detail().contains("pipeline.ail.yaml"),
            "error detail should mention the file path: {}",
            err.detail()
        );
    }

    /// Loading a file with syntactically broken YAML returns CONFIG_INVALID_YAML.
    #[test]
    fn load_invalid_yaml_returns_invalid_yaml_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bad_path = tmp.path().join("bad.ail.yaml");
        std::fs::write(&bad_path, "{ this is: [not valid yaml: at all }").unwrap();

        let err = load(&bad_path).unwrap_err();
        assert_eq!(
            err.error_type(),
            error_types::CONFIG_INVALID_YAML,
            "wrong error_type: got {}",
            err.error_type()
        );
    }

    /// Loading a file with structurally valid YAML but missing required fields
    /// (e.g. no `version`) returns a validation error.
    #[test]
    fn load_yaml_missing_version_returns_validation_error() {
        let result = load(&fixtures_dir().join("invalid_no_version.ail.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.error_type() == error_types::CONFIG_VALIDATION_FAILED
                || err.to_string().contains("version"),
            "expected a validation error mentioning version, got: {}",
            err
        );
    }

    /// Loading a pipeline with duplicate step ids returns a validation error.
    #[test]
    fn load_duplicate_step_ids_returns_validation_error() {
        let result = load(&fixtures_dir().join("invalid_duplicate_ids.ail.yaml"));
        assert!(result.is_err());
    }

    /// Pipeline::passthrough() can be constructed without any file.
    #[test]
    fn passthrough_pipeline_has_one_invocation_step() {
        let pipeline = Pipeline::passthrough();
        assert_eq!(pipeline.steps.len(), 1);
        assert_eq!(pipeline.steps[0].id.as_str(), "invocation");
        assert!(pipeline.source.is_none(), "passthrough source must be None");
    }
}
