#![allow(clippy::result_large_err)]
// Removed in M5 when `UrlSource` starts calling into this module.
#![cfg_attr(not(test), allow(dead_code))]

//! Pure HTTP fetch primitive and file-path safety checks shared by `UrlSource`.
//!
//! No orchestration lives here — just `fetch_with_cap` (one GET, byte cap
//! enforced on the body stream) and `validate_relative_file_path` (rejects
//! traversal, absolute paths, backslashes, `.`/`..` segments, null bytes, and
//! stray trailing slashes before any file is ever written to disk).

use ail_core::error::AilError;
use std::io::Read;
use std::time::Duration;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Build a `ureq::Agent` with the default timeouts used by the URL template source.
pub fn default_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .build()
}

/// GET `url` and read the body, aborting if it exceeds `max_bytes`.
///
/// Errors are mapped to `AilError::InitFailed` with detail prefixes that
/// downstream code can pattern-match for diagnostics:
/// - `transport:` — connection, DNS, timeout, unknown HTTP error
/// - `not-found:` — HTTP 404
/// - `rate-limited:` — HTTP 403/429 with a `retry-after` or `x-ratelimit-*` header
/// - `limit-exceeded:` — body grew past `max_bytes`
pub fn fetch_with_cap(
    agent: &ureq::Agent,
    url: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, AilError> {
    let response = agent.get(url).call().map_err(|e| map_ureq_error(url, e))?;
    read_capped(response.into_reader(), url, max_bytes)
}

fn read_capped<R: Read>(reader: R, url: &str, max_bytes: usize) -> Result<Vec<u8>, AilError> {
    // `take(max_bytes + 1)` lets us distinguish "body fit within cap" from
    // "body exceeded cap" in a single read pass without buffering the overflow.
    let mut limited = reader.take(max_bytes as u64 + 1);
    let mut buf = Vec::with_capacity(max_bytes.min(64 * 1024));
    limited
        .read_to_end(&mut buf)
        .map_err(|e| init(format!("transport: read failed for {url}: {e}")))?;
    if buf.len() > max_bytes {
        return Err(init(format!(
            "limit-exceeded: body from {url} exceeds {max_bytes} bytes"
        )));
    }
    Ok(buf)
}

fn map_ureq_error(url: &str, err: ureq::Error) -> AilError {
    match err {
        ureq::Error::Status(code, resp) => {
            let is_rate_limited = code == 429
                || (code == 403
                    && (resp.has("retry-after")
                        || resp.has("x-ratelimit-remaining")
                        || resp.has("x-ratelimit-reset")));
            let body = resp.into_string().unwrap_or_default();
            let body_preview = preview(&body);
            if code == 404 {
                return init(format!("not-found: HTTP 404 at {url}"));
            }
            if is_rate_limited {
                return init(format!("rate-limited: HTTP {code} at {url}{body_preview}"));
            }
            init(format!("transport: HTTP {code} at {url}{body_preview}"))
        }
        ureq::Error::Transport(t) => init(format!("transport: {url}: {t}")),
    }
}

fn preview(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let short: String = trimmed.chars().take(160).collect();
    format!(" — {short}")
}

fn init(detail: impl Into<String>) -> AilError {
    AilError::init_failed(detail.into())
}

// ── Path safety ──────────────────────────────────────────────────────────────

/// Validate that a relative file path declared by a URL manifest is safe to
/// fetch + write. Rejects traversal, absolute paths, backslashes, dot segments,
/// empty segments, null bytes, and trailing slashes.
///
/// This is the single choke point — every URL-sourced file path must pass
/// through here before we form a fetch URL or a disk path for it.
pub fn validate_relative_file_path(path: &str) -> Result<(), AilError> {
    if path.is_empty() {
        return Err(init("files-unsafe: empty path"));
    }
    if path.contains('\0') {
        return Err(init(format!("files-unsafe: `{path}` contains a null byte")));
    }
    if path.contains('\\') {
        return Err(init(format!(
            "files-unsafe: `{path}` contains a backslash (use `/` separators)"
        )));
    }
    if path.starts_with('/') {
        return Err(init(format!(
            "files-unsafe: `{path}` is absolute (must be relative)"
        )));
    }
    if path.ends_with('/') {
        return Err(init(format!(
            "files-unsafe: `{path}` ends with `/` (must be a file, not a directory)"
        )));
    }
    // Windows drive prefix (`C:\foo`, `C:/foo`): the backslash check catches the
    // first form; catch the second form by rejecting `:` in the first segment.
    if let Some(first_seg) = path.split('/').next() {
        if first_seg.contains(':') {
            return Err(init(format!(
                "files-unsafe: `{path}` contains `:` in its first segment"
            )));
        }
    }
    for seg in path.split('/') {
        if seg.is_empty() {
            return Err(init(format!("files-unsafe: `{path}` has an empty segment")));
        }
        if seg == "." || seg == ".." {
            return Err(init(format!(
                "files-unsafe: `{path}` contains a `.` or `..` segment"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_relative_file_path ─────────────────────────────────────────

    #[test]
    fn accepts_simple_file() {
        assert!(validate_relative_file_path("default.yaml").is_ok());
    }

    #[test]
    fn accepts_nested_file() {
        assert!(validate_relative_file_path("agents/atlas.ail.yaml").is_ok());
    }

    #[test]
    fn rejects_empty() {
        let err = validate_relative_file_path("").unwrap_err();
        assert!(err.detail().starts_with("files-unsafe:"));
    }

    #[test]
    fn rejects_absolute() {
        let err = validate_relative_file_path("/etc/passwd").unwrap_err();
        assert!(err.detail().contains("absolute"));
    }

    #[test]
    fn rejects_dot_dot_segment() {
        for bad in ["..", "../foo", "a/../b", "a/.."] {
            let err = validate_relative_file_path(bad).unwrap_err();
            assert!(
                err.detail().contains("`.` or `..`"),
                "expected dot-segment rejection for `{bad}`, got: {}",
                err.detail()
            );
        }
    }

    #[test]
    fn rejects_single_dot_segment() {
        for bad in [".", "./foo", "a/./b"] {
            let err = validate_relative_file_path(bad).unwrap_err();
            assert!(err.detail().contains("`.` or `..`"));
        }
    }

    #[test]
    fn rejects_empty_segment() {
        let err = validate_relative_file_path("a//b").unwrap_err();
        assert!(err.detail().contains("empty segment"));
    }

    #[test]
    fn rejects_trailing_slash() {
        let err = validate_relative_file_path("a/b/").unwrap_err();
        assert!(err.detail().contains("ends with `/`"));
    }

    #[test]
    fn rejects_backslash() {
        let err = validate_relative_file_path("a\\b").unwrap_err();
        assert!(err.detail().contains("backslash"));
    }

    #[test]
    fn rejects_null_byte() {
        let err = validate_relative_file_path("a\0b").unwrap_err();
        assert!(err.detail().contains("null byte"));
    }

    #[test]
    fn rejects_windows_drive_prefix() {
        let err = validate_relative_file_path("C:/foo").unwrap_err();
        assert!(err.detail().contains("`:`"));
    }

    #[test]
    fn all_rejections_use_init_failed_error_type() {
        for bad in [
            "", "/abs", "..", "a/../b", "a//b", "a/", "a\\b", "a\0b", "C:/foo",
        ] {
            let err = validate_relative_file_path(bad).unwrap_err();
            assert_eq!(
                err.error_type(),
                ail_core::error::error_types::INIT_FAILED,
                "wrong error type for `{bad}`"
            );
            assert!(
                err.detail().starts_with("files-unsafe:"),
                "wrong detail prefix for `{bad}`: {}",
                err.detail()
            );
        }
    }

    // ── fetch_with_cap ──────────────────────────────────────────────────────

    #[test]
    fn fetch_200_returns_body() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", "/hello.yaml")
            .with_status(200)
            .with_body("hello world")
            .create();

        let agent = default_agent();
        let body = fetch_with_cap(&agent, &format!("{}/hello.yaml", server.url()), 1024).unwrap();
        assert_eq!(body, b"hello world");
        m.assert();
    }

    #[test]
    fn fetch_404_maps_to_not_found() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/missing").with_status(404).create();

        let agent = default_agent();
        let err = fetch_with_cap(&agent, &format!("{}/missing", server.url()), 1024).unwrap_err();
        assert_eq!(err.error_type(), ail_core::error::error_types::INIT_FAILED);
        assert!(err.detail().starts_with("not-found:"));
    }

    #[test]
    fn fetch_429_maps_to_rate_limited() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rl")
            .with_status(429)
            .with_header("retry-after", "60")
            .with_body("slow down")
            .create();

        let agent = default_agent();
        let err = fetch_with_cap(&agent, &format!("{}/rl", server.url()), 1024).unwrap_err();
        assert!(err.detail().starts_with("rate-limited:"));
    }

    #[test]
    fn fetch_403_with_ratelimit_header_maps_to_rate_limited() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/rl")
            .with_status(403)
            .with_header("x-ratelimit-remaining", "0")
            .with_body("API rate limit exceeded")
            .create();

        let agent = default_agent();
        let err = fetch_with_cap(&agent, &format!("{}/rl", server.url()), 1024).unwrap_err();
        assert!(err.detail().starts_with("rate-limited:"));
    }

    #[test]
    fn fetch_500_maps_to_transport() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/boom").with_status(500).create();

        let agent = default_agent();
        let err = fetch_with_cap(&agent, &format!("{}/boom", server.url()), 1024).unwrap_err();
        assert!(err.detail().starts_with("transport:"));
        assert!(err.detail().contains("500"));
    }

    #[test]
    fn fetch_body_over_cap_is_limit_exceeded() {
        let mut server = mockito::Server::new();
        let big = vec![b'x'; 2048];
        server
            .mock("GET", "/big")
            .with_status(200)
            .with_body(big)
            .create();

        let agent = default_agent();
        let err = fetch_with_cap(&agent, &format!("{}/big", server.url()), 1024).unwrap_err();
        assert!(err.detail().starts_with("limit-exceeded:"));
        assert!(err.detail().contains("1024"));
    }

    #[test]
    fn fetch_body_exactly_at_cap_succeeds() {
        let mut server = mockito::Server::new();
        let exact = vec![b'y'; 1024];
        server
            .mock("GET", "/exact")
            .with_status(200)
            .with_body(exact.clone())
            .create();

        let agent = default_agent();
        let body = fetch_with_cap(&agent, &format!("{}/exact", server.url()), 1024).unwrap();
        assert_eq!(body.len(), 1024);
        assert_eq!(body, exact);
    }

    #[test]
    fn fetch_unreachable_host_maps_to_transport() {
        let agent = default_agent();
        // Port 1 is unlikely to be bound by anything.
        let err = fetch_with_cap(&agent, "http://127.0.0.1:1/x", 1024).unwrap_err();
        assert!(err.detail().starts_with("transport:"));
    }
}
