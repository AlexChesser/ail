#![allow(clippy::result_large_err)]

use crate::manifest::{manifest_filename, parse as parse_manifest};
use crate::source::TemplateSource;
use crate::template::{Template, TemplateFile, TemplateMeta};
use ail_core::error::AilError;
use include_dir::{include_dir, Dir, DirEntry};
use std::path::Path;

static STARTER_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../demo/starter");
static SUPERPOWERS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../demo/superpowers");
static OH_MY_AIL_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../demo/oh-my-ail");

/// Compile-time-bundled templates sourced directly from `demo/<name>/`.
///
/// Edits to any pipeline file under `demo/` are picked up on the next
/// `cargo build`, keeping `demo/` the single source of truth for the
/// three seed templates.
pub struct BundledSource {
    templates: Vec<BundledTemplate>,
}

struct BundledTemplate {
    meta: TemplateMeta,
    dir: &'static Dir<'static>,
}

impl BundledSource {
    pub fn new() -> Result<Self, AilError> {
        Ok(Self {
            templates: vec![
                load_bundled(&STARTER_DIR, "demo/starter")?,
                load_bundled(&SUPERPOWERS_DIR, "demo/superpowers")?,
                load_bundled(&OH_MY_AIL_DIR, "demo/oh-my-ail")?,
            ],
        })
    }
}

impl TemplateSource for BundledSource {
    fn list(&self) -> Vec<TemplateMeta> {
        self.templates.iter().map(|t| t.meta.clone()).collect()
    }

    fn fetch(&self, name_or_alias: &str) -> Option<Template> {
        let bundled = self
            .templates
            .iter()
            .find(|t| t.meta.matches(name_or_alias))?;
        Some(Template {
            meta: bundled.meta.clone(),
            files: collect_files(bundled.dir),
        })
    }
}

fn load_bundled(
    dir: &'static Dir<'static>,
    source_label: &str,
) -> Result<BundledTemplate, AilError> {
    let manifest_path = format!("{source_label}/{}", manifest_filename());
    let manifest_file = dir.get_file(manifest_filename()).ok_or_else(|| {
        AilError::config_not_found(format!("template manifest missing at `{manifest_path}`"))
    })?;
    let manifest_str = std::str::from_utf8(manifest_file.contents()).map_err(|e| {
        AilError::config_invalid_yaml(format!(
            "template manifest at `{manifest_path}` is not valid UTF-8: {e}"
        ))
    })?;
    let parsed = parse_manifest(manifest_str, &manifest_path)?;
    Ok(BundledTemplate {
        meta: parsed.meta,
        dir,
    })
}

fn collect_files(dir: &Dir<'_>) -> Vec<TemplateFile> {
    let mut out = Vec::new();
    collect_recursive(dir, &mut out);
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    out
}

fn collect_recursive(dir: &Dir<'_>, out: &mut Vec<TemplateFile>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(file) => {
                let relative = file.path().to_path_buf();
                // Skip the manifest itself — it's metadata, not installed content.
                if relative == Path::new(manifest_filename()) {
                    continue;
                }
                out.push(TemplateFile {
                    relative_path: relative,
                    contents: file.contents().to_vec(),
                });
            }
            DirEntry::Dir(subdir) => collect_recursive(subdir, out),
        }
    }
}
