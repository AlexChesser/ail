#![allow(clippy::result_large_err)]

//! URL-based template source.
//!
//! Fetches a manifest file over HTTP, reads its `files:` list, fetches each
//! file with the same byte caps, and returns a `Template` the install plan
//! can consume unchanged.
//!
//! This module is the orchestrator; the wire/safety primitives live in
//! `crate::fetcher`. Caps (from §33.3):
//!
//! - Manifest body:      1 MiB
//! - Per-file body:      2 MiB
//! - Cumulative bytes:  16 MiB
//! - File count:        128

use crate::fetcher::{default_agent, fetch_with_cap};
use crate::manifest::{manifest_filename, parse as parse_manifest};
use crate::template::{Template, TemplateFile};
use crate::url_ref::UrlTemplateRef;
use ail_core::error::AilError;
use std::path::PathBuf;

const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_FILE_COUNT: usize = 128;

pub struct UrlSource {
    agent: ureq::Agent,
    // Always `None` in v1 — the private-repo auth seam. Adding Bearer-header
    // support later is a one-line change in `fetch_with_cap` plumbing.
    _token: Option<String>,
}

impl Default for UrlSource {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlSource {
    pub fn new() -> Self {
        Self {
            agent: default_agent(),
            _token: None,
        }
    }

    /// Fetch a template described by `r`. All HTTP errors and manifest/file
    /// validation failures surface as `AilError::InitFailed` with the same
    /// stable detail prefixes the fetcher already emits.
    pub fn fetch_url(&self, r: &UrlTemplateRef) -> Result<Template, AilError> {
        // 1. Manifest.
        let manifest_bytes = fetch_with_cap(&self.agent, &r.manifest_url, MAX_MANIFEST_BYTES)?;
        let manifest_str = std::str::from_utf8(&manifest_bytes).map_err(|e| {
            AilError::init_failed(format!(
                "manifest-invalid: manifest at {} is not valid UTF-8: {e}",
                r.manifest_url
            ))
        })?;
        let parsed = parse_manifest(manifest_str, &r.manifest_url)?;

        // 2. `files:` required for URL sources.
        let file_list = parsed.files.ok_or_else(|| {
            AilError::init_failed(format!(
                "manifest-invalid: URL-sourced manifest at {} must declare a `files:` list \
                 (see SPEC §32.5 / §33.5 for the schema) — got no `files:` field",
                r.manifest_url
            ))
        })?;

        // 3. File count cap.
        if file_list.len() > MAX_FILE_COUNT {
            return Err(AilError::init_failed(format!(
                "limit-exceeded: manifest at {} declares {} files; max is {MAX_FILE_COUNT}",
                r.manifest_url,
                file_list.len()
            )));
        }

        // 4. Fetch each file with per-file + cumulative caps.
        let mut files: Vec<TemplateFile> = Vec::with_capacity(file_list.len());
        let mut cumulative: usize = 0;
        for relative in &file_list {
            // Safety was validated at parse time, but double-check: a
            // defence-in-depth invariant we want the URL join to rely on.
            crate::fetcher::validate_relative_file_path(relative)?;
            let file_url = format!("{}{}", r.base_url, relative);
            let bytes = fetch_with_cap(&self.agent, &file_url, MAX_FILE_BYTES)?;
            cumulative = cumulative.saturating_add(bytes.len());
            if cumulative > MAX_TOTAL_BYTES {
                return Err(AilError::init_failed(format!(
                    "limit-exceeded: cumulative download for {} exceeds {MAX_TOTAL_BYTES} bytes",
                    r.label
                )));
            }
            files.push(TemplateFile {
                relative_path: PathBuf::from(relative),
                contents: bytes,
            });
        }

        // Defensive: guarantee the manifest itself is never written to disk,
        // even though `manifest::parse` already rejected it in `files:`.
        files.retain(|f| f.relative_path != std::path::Path::new(manifest_filename()));

        Ok(Template {
            meta: parsed.meta,
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_ref(manifest_url: &str, base_url: &str) -> UrlTemplateRef {
        UrlTemplateRef {
            manifest_url: manifest_url.to_string(),
            base_url: base_url.to_string(),
            label: "test".to_string(),
        }
    }

    #[test]
    fn happy_path_fetches_manifest_and_files() {
        let mut server = mockito::Server::new();
        let manifest = r#"
name: remote-template
short_description: fetched over HTTP
files:
  - default.yaml
  - agents/atlas.ail.yaml
"#;
        let default_yaml = b"version: 1\nsteps: []\n";
        let atlas_yaml = b"name: atlas\n";

        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(manifest)
            .create();
        server
            .mock("GET", "/default.yaml")
            .with_status(200)
            .with_body(default_yaml)
            .create();
        server
            .mock("GET", "/agents/atlas.ail.yaml")
            .with_status(200)
            .with_body(atlas_yaml)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let src = UrlSource::new();
        let tpl = src.fetch_url(&url_ref(&manifest_url, &base)).unwrap();

        assert_eq!(tpl.meta.name, "remote-template");
        assert_eq!(tpl.files.len(), 2);
        assert_eq!(tpl.files[0].relative_path, PathBuf::from("default.yaml"));
        assert_eq!(tpl.files[0].contents, default_yaml);
        assert_eq!(
            tpl.files[1].relative_path,
            PathBuf::from("agents/atlas.ail.yaml")
        );
        assert_eq!(tpl.files[1].contents, atlas_yaml);
    }

    #[test]
    fn manifest_404_returns_not_found() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/template.yaml")
            .with_status(404)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("not-found:"), "{}", err.detail());
    }

    #[test]
    fn manifest_missing_files_list_errors() {
        let mut server = mockito::Server::new();
        let manifest = r#"
name: no-files
short_description: oops
"#;
        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(manifest)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("manifest-invalid:"));
        assert!(err.detail().contains("files:"));
    }

    #[test]
    fn individual_file_404_returns_not_found() {
        let mut server = mockito::Server::new();
        let manifest = r#"
name: partial
short_description: x
files:
  - missing.yaml
"#;
        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(manifest)
            .create();
        server
            .mock("GET", "/missing.yaml")
            .with_status(404)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("not-found:"));
        assert!(err.detail().contains("missing.yaml"));
    }

    #[test]
    fn oversized_file_is_limit_exceeded() {
        let mut server = mockito::Server::new();
        let manifest = r#"
name: big
short_description: x
files:
  - huge.bin
"#;
        let huge = vec![b'z'; MAX_FILE_BYTES + 1];
        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(manifest)
            .create();
        server
            .mock("GET", "/huge.bin")
            .with_status(200)
            .with_body(huge)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("limit-exceeded:"));
    }

    #[test]
    fn manifest_unsafe_path_rejected_before_fetch() {
        let mut server = mockito::Server::new();
        let manifest = r#"
name: bad
short_description: x
files:
  - ../../etc/passwd
"#;
        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(manifest)
            .create();
        // Deliberately NOT mocking `../../etc/passwd` — parse-time rejection must fire first.

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("files-unsafe:"));
    }

    #[test]
    fn manifest_invalid_utf8_errors() {
        let mut server = mockito::Server::new();
        let bad = [0xFFu8, 0xFE, 0xFD, 0xFC];
        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(bad)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("manifest-invalid:"));
        assert!(err.detail().contains("UTF-8"));
    }

    #[test]
    fn too_many_files_is_limit_exceeded() {
        let mut server = mockito::Server::new();
        let mut manifest = String::from("name: many\nshort_description: x\nfiles:\n");
        for i in 0..(MAX_FILE_COUNT + 1) {
            manifest.push_str(&format!("  - f{i}.yaml\n"));
        }
        server
            .mock("GET", "/template.yaml")
            .with_status(200)
            .with_body(manifest)
            .create();

        let base = format!("{}/", server.url());
        let manifest_url = format!("{}template.yaml", base);
        let err = UrlSource::new()
            .fetch_url(&url_ref(&manifest_url, &base))
            .unwrap_err();
        assert!(err.detail().starts_with("limit-exceeded:"));
        assert!(err.detail().contains(&format!("{}", MAX_FILE_COUNT)));
    }
}
