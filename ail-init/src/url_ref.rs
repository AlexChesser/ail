#![allow(clippy::result_large_err)]
// Removed in M5 when `run_in_cwd` routes URL-shaped args through this module.
#![cfg_attr(not(test), allow(dead_code))]

//! URL-shaped template argument parsing for `ail init <URL>`.
//!
//! Recognises three argument shapes and resolves each to a pair of URLs the
//! fetcher needs:
//!
//! 1. `github:owner/repo[@ref][/subpath]` — shorthand.
//! 2. `https://github.com/owner/repo` or `.../tree/<ref>[/subpath]` — web URL.
//! 3. `https://<host>/.../template.yaml` — explicit manifest URL (escape hatch
//!    for other static hosts; raw.githubusercontent.com URLs fall into this case).
//!
//! Plain names (no `://`, no `github:` prefix) are not URL-shaped; the parser
//! returns `Ok(None)` so the caller can fall through to the bundled source.

use ail_core::error::AilError;

pub const MANIFEST_FILENAME: &str = "template.yaml";
const GITHUB_WEB_HOST: &str = "github.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlTemplateRef {
    /// Absolute URL of the template manifest (`template.yaml`).
    pub manifest_url: String,
    /// Absolute URL with trailing `/`; file paths in the manifest resolve against this.
    pub base_url: String,
    /// Short human-readable summary for diagnostics.
    pub label: String,
}

/// Parse a CLI argument. Returns:
/// - `Ok(Some(r))` when the argument is URL-shaped and parses cleanly.
/// - `Ok(None)` when the argument is a plain name (fall through to bundled).
/// - `Err(InitFailed { detail: "url-invalid: ..." })` when the argument is
///   clearly URL-shaped but malformed.
pub fn parse(arg: &str) -> Result<Option<UrlTemplateRef>, AilError> {
    let arg = arg.trim();

    if let Some(rest) = arg.strip_prefix("github:") {
        return parse_shorthand(rest).map(Some);
    }

    if arg.contains("://") {
        return parse_url(arg).map(Some);
    }

    Ok(None)
}

// ── Shorthand: github:owner/repo[@ref][/subpath] ─────────────────────────────

fn parse_shorthand(rest: &str) -> Result<UrlTemplateRef, AilError> {
    if rest.is_empty() {
        return Err(url_invalid("github: shorthand requires `owner/repo`"));
    }

    // Split optional subpath on the first `/` after `owner/repo` (or after `@ref`).
    let (owner_repo_ref, subpath) = split_after_owner_repo(rest)?;

    // owner_repo_ref is `owner/repo` or `owner/repo@ref`.
    let (owner_repo, ref_name) = match owner_repo_ref.split_once('@') {
        Some((or, r)) => (or, r),
        None => (owner_repo_ref, "HEAD"),
    };

    let (owner, repo) = split_owner_repo(owner_repo)?;
    validate_ref(ref_name)?;
    if let Some(ref sp) = subpath {
        validate_subpath(sp)?;
    }

    Ok(build_github_ref(owner, repo, ref_name, subpath.as_deref()))
}

/// Split `owner/repo[@ref]/subpath` into `(owner_repo_ref, Option<subpath>)`.
/// Subpath starts at the third `/` segment (after `owner` and `repo`).
fn split_after_owner_repo(s: &str) -> Result<(&str, Option<String>), AilError> {
    let mut slashes = s.match_indices('/');
    let _first = slashes
        .next()
        .ok_or_else(|| url_invalid("github: shorthand requires `owner/repo`"))?;
    match slashes.next() {
        Some((idx, _)) => {
            let (left, right) = s.split_at(idx);
            // right starts with `/` — strip it
            let sub = &right[1..];
            if sub.is_empty() {
                return Err(url_invalid("github: shorthand has a trailing `/`"));
            }
            Ok((left, Some(sub.to_string())))
        }
        None => Ok((s, None)),
    }
}

// ── Full URL form ────────────────────────────────────────────────────────────

fn parse_url(url: &str) -> Result<UrlTemplateRef, AilError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| url_invalid(format!("`{url}` is not a valid URL (missing scheme)")))?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return Err(url_invalid(format!(
            "unsupported URL scheme `{scheme}` — only http/https are accepted"
        )));
    }
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host.is_empty() {
        return Err(url_invalid(format!("`{url}` is missing a host")));
    }

    // GitHub web URL → rewrite into raw.githubusercontent.com.
    if host.eq_ignore_ascii_case(GITHUB_WEB_HOST) {
        return parse_github_web_path(path);
    }

    // Otherwise — escape hatch: the URL must point directly at a template.yaml.
    parse_generic_manifest_url(url)
}

fn parse_github_web_path(path: &str) -> Result<UrlTemplateRef, AilError> {
    let trimmed = path.trim_end_matches('/');
    let mut segs = trimmed.split('/').filter(|s| !s.is_empty());
    let owner = segs
        .next()
        .ok_or_else(|| url_invalid("GitHub URL is missing the owner segment"))?;
    let repo = segs
        .next()
        .ok_or_else(|| url_invalid("GitHub URL is missing the repo segment"))?;
    let owner = validate_owner_or_repo(owner, "owner")?;
    let repo = validate_owner_or_repo(repo, "repo")?;

    let (ref_name, subpath) = match segs.next() {
        None => ("HEAD".to_string(), None),
        Some("tree") => {
            let r = segs
                .next()
                .ok_or_else(|| url_invalid("GitHub `/tree/` URL is missing the ref segment"))?;
            validate_ref(r)?;
            let rest: Vec<&str> = segs.collect();
            let sp = if rest.is_empty() {
                None
            } else {
                let joined = rest.join("/");
                validate_subpath(&joined)?;
                Some(joined)
            };
            (r.to_string(), sp)
        }
        Some(other) => {
            return Err(url_invalid(format!(
            "unsupported GitHub URL path — expected `/tree/<ref>[/<subpath>]`, got `/{other}/...`"
        )))
        }
    };

    Ok(build_github_ref(owner, repo, &ref_name, subpath.as_deref()))
}

/// Fallback for arbitrary `https://<host>/.../template.yaml` URLs.
fn parse_generic_manifest_url(url: &str) -> Result<UrlTemplateRef, AilError> {
    let (before_query, _) = url.split_once('?').unwrap_or((url, ""));
    let (before_frag, _) = before_query.split_once('#').unwrap_or((before_query, ""));
    let trimmed = before_frag;

    if !trimmed.ends_with(&format!("/{MANIFEST_FILENAME}")) {
        return Err(url_invalid(format!(
            "non-GitHub URLs must point at a `{MANIFEST_FILENAME}` file \
             (got `{url}`)"
        )));
    }
    // base_url = everything up to and including the trailing slash before the manifest.
    let base_end = trimmed.len() - MANIFEST_FILENAME.len();
    let base = &trimmed[..base_end];
    let label = format!("manifest at {}", trimmed);
    Ok(UrlTemplateRef {
        manifest_url: trimmed.to_string(),
        base_url: base.to_string(),
        label,
    })
}

// ── Shared helpers ───────────────────────────────────────────────────────────

fn split_owner_repo(s: &str) -> Result<(&str, &str), AilError> {
    let (owner, repo) = s
        .split_once('/')
        .ok_or_else(|| url_invalid("github: shorthand requires `owner/repo` (missing `/`)"))?;
    let owner = validate_owner_or_repo(owner, "owner")?;
    let repo = validate_owner_or_repo(repo, "repo")?;
    Ok((owner, repo))
}

fn validate_owner_or_repo<'a>(s: &'a str, field: &str) -> Result<&'a str, AilError> {
    if s.is_empty() {
        return Err(url_invalid(format!("GitHub {field} segment is empty")));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(url_invalid(format!(
            "GitHub {field} `{s}` contains invalid characters \
             (allowed: alphanumerics, `-`, `_`, `.`)"
        )));
    }
    Ok(s)
}

fn validate_ref(r: &str) -> Result<(), AilError> {
    // Refs with `/` (e.g. `feature/foo`) can't be expressed in either the shorthand
    // or the `/tree/<ref>` URL form — both split on `/`. Users who need them can
    // point directly at `raw.githubusercontent.com/.../template.yaml` (the generic
    // manifest URL path).
    if r.is_empty() {
        return Err(url_invalid("ref is empty"));
    }
    if r.contains("..") {
        return Err(url_invalid(format!("ref `{r}` contains `..`")));
    }
    Ok(())
}

fn validate_subpath(sp: &str) -> Result<(), AilError> {
    if sp.starts_with('/') {
        return Err(url_invalid(format!(
            "subpath `{sp}` must not start with `/`"
        )));
    }
    if sp.ends_with('/') {
        return Err(url_invalid(format!("subpath `{sp}` must not end with `/`")));
    }
    for seg in sp.split('/') {
        if seg.is_empty() {
            return Err(url_invalid(format!("subpath `{sp}` has empty segments")));
        }
        if seg == ".." || seg == "." {
            return Err(url_invalid(format!(
                "subpath `{sp}` must not contain `.` or `..` segments"
            )));
        }
    }
    Ok(())
}

fn build_github_ref(
    owner: &str,
    repo: &str,
    ref_name: &str,
    subpath: Option<&str>,
) -> UrlTemplateRef {
    let base_url = match subpath {
        Some(sp) => format!("https://raw.githubusercontent.com/{owner}/{repo}/{ref_name}/{sp}/"),
        None => format!("https://raw.githubusercontent.com/{owner}/{repo}/{ref_name}/"),
    };
    let manifest_url = format!("{base_url}{MANIFEST_FILENAME}");
    let label = match subpath {
        Some(sp) => format!("{owner}/{repo}@{ref_name}/{sp}"),
        None => format!("{owner}/{repo}@{ref_name}"),
    };
    UrlTemplateRef {
        manifest_url,
        base_url,
        label,
    }
}

fn url_invalid(detail: impl Into<String>) -> AilError {
    AilError::init_failed(format!("url-invalid: {}", detail.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_github(
        r: &UrlTemplateRef,
        owner: &str,
        repo: &str,
        ref_name: &str,
        subpath: Option<&str>,
    ) {
        let expected_base = match subpath {
            Some(sp) => {
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{ref_name}/{sp}/")
            }
            None => format!("https://raw.githubusercontent.com/{owner}/{repo}/{ref_name}/"),
        };
        assert_eq!(r.base_url, expected_base, "base_url mismatch");
        assert_eq!(
            r.manifest_url,
            format!("{expected_base}template.yaml"),
            "manifest_url mismatch"
        );
    }

    // ── Pass-through ────────────────────────────────────────────────────────

    #[test]
    fn plain_name_is_pass_through() {
        assert_eq!(parse("starter").unwrap(), None);
        assert_eq!(parse("oh-my-ail").unwrap(), None);
        assert_eq!(parse("oma").unwrap(), None);
    }

    #[test]
    fn owner_slash_repo_is_pass_through_not_url() {
        // Bare `owner/repo` is deliberately NOT recognised — it would collide
        // with future template names.
        assert_eq!(parse("foo/bar").unwrap(), None);
    }

    // ── Shorthand ───────────────────────────────────────────────────────────

    #[test]
    fn shorthand_owner_repo_defaults_to_head() {
        let r = parse("github:alice/my-template").unwrap().unwrap();
        assert_github(&r, "alice", "my-template", "HEAD", None);
    }

    #[test]
    fn shorthand_with_ref() {
        let r = parse("github:alice/my-template@v1.2.3").unwrap().unwrap();
        assert_github(&r, "alice", "my-template", "v1.2.3", None);
    }

    #[test]
    fn shorthand_with_subpath_and_no_ref() {
        let r = parse("github:alice/my-repo/templates/basic")
            .unwrap()
            .unwrap();
        assert_github(&r, "alice", "my-repo", "HEAD", Some("templates/basic"));
    }

    #[test]
    fn shorthand_with_ref_and_subpath() {
        let r = parse("github:alice/my-repo@v1/templates/basic")
            .unwrap()
            .unwrap();
        assert_github(&r, "alice", "my-repo", "v1", Some("templates/basic"));
    }

    #[test]
    fn shorthand_empty_after_prefix_errors() {
        let err = parse("github:").unwrap_err();
        assert_eq!(err.error_type(), ail_core::error::error_types::INIT_FAILED);
        assert!(err.detail().starts_with("url-invalid:"));
    }

    #[test]
    fn shorthand_missing_repo_errors() {
        let err = parse("github:alice").unwrap_err();
        assert!(err.detail().contains("owner/repo"));
    }

    #[test]
    fn shorthand_empty_owner_errors() {
        let err = parse("github:/repo").unwrap_err();
        assert!(err.detail().contains("owner"));
    }

    #[test]
    fn shorthand_invalid_owner_chars_error() {
        let err = parse("github:alice!/repo").unwrap_err();
        assert!(err.detail().contains("invalid characters"));
    }

    #[test]
    fn shorthand_trailing_slash_errors() {
        let err = parse("github:alice/repo/").unwrap_err();
        assert!(err.detail().contains("trailing `/`"));
    }

    #[test]
    fn shorthand_subpath_with_dotdot_errors() {
        let err = parse("github:alice/repo@v1/../evil").unwrap_err();
        assert!(err.detail().contains("must not contain"));
    }

    #[test]
    fn shorthand_ref_with_slash_is_split_as_subpath() {
        // `github:owner/repo@feature/foo` — neither grammar can distinguish a
        // slash-in-ref from a subpath boundary, so `feature` becomes the ref and
        // `foo` the subpath. Users who genuinely need slashed refs point at the
        // raw manifest URL directly.
        let r = parse("github:alice/repo@feature/foo").unwrap().unwrap();
        assert_github(&r, "alice", "repo", "feature", Some("foo"));
    }

    // ── GitHub web URL ──────────────────────────────────────────────────────

    #[test]
    fn github_web_root_defaults_to_head() {
        let r = parse("https://github.com/alice/my-repo").unwrap().unwrap();
        assert_github(&r, "alice", "my-repo", "HEAD", None);
    }

    #[test]
    fn github_web_tree_ref_only() {
        let r = parse("https://github.com/alice/my-repo/tree/v2")
            .unwrap()
            .unwrap();
        assert_github(&r, "alice", "my-repo", "v2", None);
    }

    #[test]
    fn github_web_tree_ref_and_subpath() {
        let r = parse("https://github.com/alice/my-repo/tree/main/templates/basic")
            .unwrap()
            .unwrap();
        assert_github(&r, "alice", "my-repo", "main", Some("templates/basic"));
    }

    #[test]
    fn github_web_trailing_slash_ok() {
        let r = parse("https://github.com/alice/my-repo/").unwrap().unwrap();
        assert_github(&r, "alice", "my-repo", "HEAD", None);
    }

    #[test]
    fn github_web_unknown_second_segment_errors() {
        let err = parse("https://github.com/alice/my-repo/blob/main/file.yaml").unwrap_err();
        assert!(err.detail().contains("expected `/tree/"));
    }

    #[test]
    fn github_web_missing_ref_after_tree_errors() {
        let err = parse("https://github.com/alice/my-repo/tree").unwrap_err();
        assert!(err.detail().contains("ref segment"));
    }

    // ── Raw / generic manifest URL ──────────────────────────────────────────

    #[test]
    fn raw_manifest_url_passes_through() {
        let url =
            "https://raw.githubusercontent.com/alice/my-repo/v1/templates/basic/template.yaml";
        let r = parse(url).unwrap().unwrap();
        assert_eq!(r.manifest_url, url);
        assert_eq!(
            r.base_url,
            "https://raw.githubusercontent.com/alice/my-repo/v1/templates/basic/"
        );
    }

    #[test]
    fn generic_host_manifest_url_accepted() {
        let url = "https://example.com/my/path/template.yaml";
        let r = parse(url).unwrap().unwrap();
        assert_eq!(r.manifest_url, url);
        assert_eq!(r.base_url, "https://example.com/my/path/");
    }

    #[test]
    fn generic_url_without_manifest_filename_errors() {
        let err = parse("https://example.com/my/path/").unwrap_err();
        assert!(err.detail().contains("template.yaml"));
    }

    #[test]
    fn non_http_scheme_errors() {
        let err = parse("ftp://example.com/template.yaml").unwrap_err();
        assert!(err.detail().contains("scheme"));
    }

    #[test]
    fn missing_host_errors() {
        let err = parse("https:///path/template.yaml").unwrap_err();
        assert!(err.detail().contains("host"));
    }

    // ── All error paths set the right error_type ───────────────────────────

    #[test]
    fn all_errors_use_init_failed_type() {
        for bad in [
            "github:",
            "github:alice",
            "github:alice!/repo",
            "https://github.com/alice/repo/blob/main/x.yaml",
            "https://example.com/not-a-manifest",
            "ftp://example.com/template.yaml",
        ] {
            let err = parse(bad).unwrap_err();
            assert_eq!(
                err.error_type(),
                ail_core::error::error_types::INIT_FAILED,
                "error_type wrong for input `{bad}`"
            );
            assert!(
                err.detail().starts_with("url-invalid:"),
                "detail prefix wrong for input `{bad}`: {}",
                err.detail()
            );
        }
    }
}
