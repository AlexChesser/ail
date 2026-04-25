#![allow(clippy::result_large_err)]

use crate::fetcher::validate_relative_file_path;
use crate::template::TemplateMeta;
use ail_core::error::AilError;
use serde::Deserialize;
use std::collections::HashSet;

const MANIFEST_FILENAME: &str = "template.yaml";

pub(crate) const fn manifest_filename() -> &'static str {
    MANIFEST_FILENAME
}

#[derive(Debug, Deserialize)]
struct ManifestDto {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    short_description: String,
    #[serde(default)]
    tags: Vec<String>,
    // Optional. Required by URL-sourced manifests (enforced by `UrlSource`);
    // bundled manifests omit it and the directory walker supplies the files.
    #[serde(default)]
    files: Option<Vec<String>>,
}

/// Parsed manifest — metadata plus the optional URL-source file list.
#[derive(Debug, Clone)]
pub(crate) struct ParsedManifest {
    pub meta: TemplateMeta,
    pub files: Option<Vec<String>>,
}

pub(crate) fn parse(contents: &str, source: &str) -> Result<ParsedManifest, AilError> {
    let dto: ManifestDto = serde_yaml::from_str(contents).map_err(|e| {
        AilError::config_invalid_yaml(format!(
            "failed to parse template manifest at `{source}`: {e}"
        ))
    })?;

    if dto.name.trim().is_empty() {
        return Err(AilError::config_validation(format!(
            "template manifest at `{source}` declares an empty `name`"
        )));
    }

    if let Some(files) = dto.files.as_ref() {
        validate_files_list(files, source)?;
    }

    Ok(ParsedManifest {
        meta: TemplateMeta {
            name: dto.name,
            aliases: dto.aliases,
            short_description: dto.short_description,
            tags: dto.tags,
        },
        files: dto.files,
    })
}

fn validate_files_list(files: &[String], source: &str) -> Result<(), AilError> {
    if files.is_empty() {
        return Err(AilError::init_failed(format!(
            "manifest-invalid: template manifest at `{source}` declares an empty `files:` list"
        )));
    }
    let mut seen: HashSet<&str> = HashSet::with_capacity(files.len());
    for entry in files {
        validate_relative_file_path(entry)?;
        if entry == MANIFEST_FILENAME {
            return Err(AilError::init_failed(format!(
                "files-unsafe: `{MANIFEST_FILENAME}` cannot appear in its own `files:` list \
                 (manifests are metadata, never installed content) — see `{source}`"
            )));
        }
        if !seen.insert(entry.as_str()) {
            return Err(AilError::init_failed(format!(
                "manifest-invalid: duplicate entry `{entry}` in `files:` at `{source}`"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest() {
        let yaml = r#"
name: starter
short_description: Minimal pipeline
"#;
        let m = parse(yaml, "test").unwrap();
        assert_eq!(m.meta.name, "starter");
        assert!(m.meta.aliases.is_empty());
        assert_eq!(m.meta.short_description, "Minimal pipeline");
        assert!(m.meta.tags.is_empty());
        assert!(m.files.is_none());
    }

    #[test]
    fn parses_full_manifest() {
        let yaml = r#"
name: oh-my-ail
aliases: [oma, omai]
short_description: Multi-agent orchestration
tags: [advanced, multi-agent]
"#;
        let m = parse(yaml, "test").unwrap();
        assert_eq!(m.meta.name, "oh-my-ail");
        assert_eq!(m.meta.aliases, vec!["oma", "omai"]);
        assert_eq!(m.meta.tags, vec!["advanced", "multi-agent"]);
        assert!(m.files.is_none());
    }

    #[test]
    fn rejects_empty_name() {
        let yaml = r#"
name: "   "
short_description: x
"#;
        let err = parse(yaml, "test").unwrap_err();
        assert!(err.detail().contains("empty `name`"));
    }

    #[test]
    fn rejects_bad_yaml() {
        let err = parse("not: [valid: yaml", "test").unwrap_err();
        assert!(err.detail().contains("failed to parse"));
    }

    #[test]
    fn parses_files_list_when_present() {
        let yaml = r#"
name: remote-template
short_description: From a URL
files:
  - default.yaml
  - README.md
  - agents/atlas.ail.yaml
"#;
        let m = parse(yaml, "test").unwrap();
        let files = m.files.unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], "default.yaml");
        assert_eq!(files[2], "agents/atlas.ail.yaml");
    }

    #[test]
    fn rejects_empty_files_list() {
        let yaml = r#"
name: x
short_description: y
files: []
"#;
        let err = parse(yaml, "test").unwrap_err();
        assert!(err.detail().starts_with("manifest-invalid:"));
        assert!(err.detail().contains("empty `files:`"));
    }

    #[test]
    fn rejects_files_with_absolute_path() {
        let yaml = r#"
name: x
short_description: y
files:
  - /etc/passwd
"#;
        let err = parse(yaml, "test").unwrap_err();
        assert!(err.detail().starts_with("files-unsafe:"));
    }

    #[test]
    fn rejects_files_with_traversal() {
        let yaml = r#"
name: x
short_description: y
files:
  - a/../b.yaml
"#;
        let err = parse(yaml, "test").unwrap_err();
        assert!(err.detail().starts_with("files-unsafe:"));
    }

    #[test]
    fn rejects_template_yaml_in_files_list() {
        let yaml = r#"
name: x
short_description: y
files:
  - default.yaml
  - template.yaml
"#;
        let err = parse(yaml, "test").unwrap_err();
        assert!(err.detail().contains("template.yaml"));
        assert!(err.detail().contains("never installed content"));
    }

    #[test]
    fn rejects_duplicate_file_entry() {
        let yaml = r#"
name: x
short_description: y
files:
  - default.yaml
  - README.md
  - default.yaml
"#;
        let err = parse(yaml, "test").unwrap_err();
        assert!(err.detail().contains("duplicate"));
    }
}
