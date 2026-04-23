#![allow(clippy::result_large_err)]

use crate::template::TemplateMeta;
use ail_core::error::AilError;
use serde::Deserialize;

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
}

pub(crate) fn parse(contents: &str, source: &str) -> Result<TemplateMeta, AilError> {
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

    Ok(TemplateMeta {
        name: dto.name,
        aliases: dto.aliases,
        short_description: dto.short_description,
        tags: dto.tags,
    })
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
        let meta = parse(yaml, "test").unwrap();
        assert_eq!(meta.name, "starter");
        assert!(meta.aliases.is_empty());
        assert_eq!(meta.short_description, "Minimal pipeline");
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn parses_full_manifest() {
        let yaml = r#"
name: oh-my-ail
aliases: [oma, omai]
short_description: Multi-agent orchestration
tags: [advanced, multi-agent]
"#;
        let meta = parse(yaml, "test").unwrap();
        assert_eq!(meta.name, "oh-my-ail");
        assert_eq!(meta.aliases, vec!["oma", "omai"]);
        assert_eq!(meta.tags, vec!["advanced", "multi-agent"]);
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
}
