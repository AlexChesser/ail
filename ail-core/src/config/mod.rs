#![allow(clippy::result_large_err)]

pub mod discovery;
pub mod domain;
pub use discovery::PipelineEntry;
pub mod dto;
pub mod validation;

use std::path::Path;

use crate::error::{error_types, AilError};
use domain::Pipeline;
use dto::PipelineFileDto;
use validation::validate;

pub fn load(path: &Path) -> Result<Pipeline, AilError> {
    let contents = std::fs::read_to_string(path).map_err(|e| AilError {
        error_type: error_types::CONFIG_FILE_NOT_FOUND,
        title: "Pipeline file not found",
        detail: format!("Could not read '{}': {e}", path.display()),
        context: None,
    })?;

    let dto: PipelineFileDto = serde_yaml::from_str(&contents).map_err(|e| AilError {
        error_type: error_types::CONFIG_INVALID_YAML,
        title: "Invalid YAML",
        detail: format!("Failed to parse '{}': {e}", path.display()),
        context: None,
    })?;

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

    validate(dto, abs_source)
}
