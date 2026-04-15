//! Parses `/PATTERN/FLAGS` regex literals per SPEC §12.3.
//!
//! Regex literals use conventional JavaScript/Perl/Ruby-style syntax:
//! a leading `/`, the pattern, a closing `/`, and zero or more flag
//! characters. The parser compiles the regex at parse time, so invalid
//! patterns and unsupported flags fail at pipeline load — never at match
//! time.

#![allow(clippy::result_large_err)]

use regex::{Regex, RegexBuilder};

/// A parsed regex literal with both the compiled form and the original source.
///
/// `source` is preserved for error messages and materialize output so the
/// literal the user wrote can be surfaced verbatim rather than reconstructed.
#[derive(Debug, Clone)]
pub struct ParsedRegex {
    /// Original source, e.g. `/warn|error/i`.
    pub source: String,
    /// Compiled regex.
    pub regex: Regex,
}

/// Parse a `/PATTERN/FLAGS` literal per SPEC §12.3.
///
/// - Delimiters are the *first* `/` and the *last* `/` followed by zero or
///   more flag characters (`[ims]*`) at end-of-string.
/// - Supported flags: `i` (case-insensitive), `m` (multiline), `s` (dotall).
/// - `g` is rejected explicitly — matching is boolean, so "global" is
///   meaningless.
/// - Other trailing letters are rejected as unsupported flags.
/// - `\/` inside the pattern is unescaped to a literal `/` before compilation.
///
/// On parse or compile failure, returns a human-readable diagnostic string
/// suitable for embedding in a `CONFIG_VALIDATION_FAILED` error.
pub fn parse_regex_literal(raw: &str) -> Result<ParsedRegex, String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('/') {
        return Err(format!(
            "regex literal must start with '/', got '{raw}' (SPEC §12.3)"
        ));
    }
    if trimmed.len() < 2 {
        return Err(format!(
            "regex literal '{raw}' is unterminated (missing closing '/')"
        ));
    }

    // Scan backward for the last '/' whose trailing characters are all valid
    // flag characters. Bytes are safe because '/' is ASCII — a multi-byte
    // codepoint never starts with 0x2F.
    let bytes = trimmed.as_bytes();
    let mut end: Option<usize> = None;
    for i in (1..bytes.len()).rev() {
        if bytes[i] != b'/' {
            continue;
        }
        let tail = &trimmed[i + 1..];
        if tail.chars().all(|c| matches!(c, 'i' | 'm' | 's')) {
            end = Some(i);
            break;
        }
        // If the tail looks like it's meant to be flag characters (all ASCII
        // letters) but contains an unsupported one, give a specific error.
        // This catches `/pat/gi`, `/pat/x`, etc.
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_alphabetic()) {
            if tail.contains('g') {
                return Err(format!(
                    "regex '{raw}' uses the 'g' flag, which is not meaningful \
                     for boolean matching in ail — matching is match/no-match, \
                     not iterative. Remove the flag. (SPEC §12.3)"
                ));
            }
            let bad: String = tail
                .chars()
                .filter(|c| !matches!(c, 'i' | 'm' | 's'))
                .collect();
            return Err(format!(
                "regex '{raw}' uses unsupported flag(s): '{bad}'. Only 'i', 'm', 's' \
                 are accepted as trailing flags. Use inline flag syntax like (?x) \
                 inside the pattern for other modes. (SPEC §12.3)"
            ));
        }
        // Otherwise, this '/' is inside the pattern — keep scanning backward.
    }

    let end = end.ok_or_else(|| {
        format!("regex literal '{raw}' is not terminated (missing closing '/') (SPEC §12.3)")
    })?;

    let pattern_raw = &trimmed[1..end];
    let flags = &trimmed[end + 1..];

    if pattern_raw.is_empty() {
        return Err(format!(
            "regex literal '{raw}' has an empty pattern (SPEC §12.3)"
        ));
    }

    // Unescape `\/` → `/`. Any other backslash sequence is left alone so the
    // regex engine sees it verbatim (e.g. `\d`, `\b`, `\s`).
    let pattern = unescape_forward_slashes(pattern_raw);

    let mut builder = RegexBuilder::new(&pattern);
    for ch in flags.chars() {
        match ch {
            'i' => {
                builder.case_insensitive(true);
            }
            'm' => {
                builder.multi_line(true);
            }
            's' => {
                builder.dot_matches_new_line(true);
            }
            _ => unreachable!("flag chars restricted by scan loop"),
        }
    }

    let regex = builder
        .build()
        .map_err(|e| format!("regex '{raw}' failed to compile: {e} (SPEC §12.3)"))?;

    Ok(ParsedRegex {
        source: trimmed.to_string(),
        regex,
    })
}

/// Replace `\/` with `/` in a pattern body. Other escape sequences are left
/// untouched so the regex engine sees them verbatim.
fn unescape_forward_slashes(s: &str) -> String {
    // Fast path — most patterns don't contain `\/`.
    if !s.contains("\\/") {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'/') {
            out.push('/');
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_literal() {
        let r = parse_regex_literal("/warn|error/").unwrap();
        assert_eq!(r.source, "/warn|error/");
        assert!(r.regex.is_match("warning"));
        assert!(r.regex.is_match("errors here"));
        assert!(!r.regex.is_match("ok"));
    }

    #[test]
    fn parses_literal_with_i_flag() {
        let r = parse_regex_literal("/pass/i").unwrap();
        assert!(r.regex.is_match("PASS"));
        assert!(r.regex.is_match("passed"));
    }

    #[test]
    fn parses_literal_with_m_flag() {
        let r = parse_regex_literal("/^foo/m").unwrap();
        assert!(r.regex.is_match("bar\nfoo"));
    }

    #[test]
    fn parses_literal_with_s_flag() {
        let r = parse_regex_literal("/a.b/s").unwrap();
        assert!(r.regex.is_match("a\nb"));
    }

    #[test]
    fn parses_literal_with_multiple_flags() {
        let r = parse_regex_literal("/^pass$/im").unwrap();
        assert!(r.regex.is_match("warning\nPASS"));
    }

    #[test]
    fn unanchored_by_default() {
        let r = parse_regex_literal("/PASS/").unwrap();
        assert!(r.regex.is_match("tests PASSED"));
    }

    #[test]
    fn case_sensitive_by_default() {
        let r = parse_regex_literal("/PASS/").unwrap();
        assert!(!r.regex.is_match("pass"));
    }

    #[test]
    fn anchors_work() {
        let r = parse_regex_literal("/^PASS$/").unwrap();
        assert!(r.regex.is_match("PASS"));
        assert!(!r.regex.is_match("PASSED"));
    }

    #[test]
    fn patterns_with_embedded_slashes() {
        // The last `/` followed by no flags delimits — so `a/b` is pattern.
        let r = parse_regex_literal("/a/b/").unwrap();
        assert!(r.regex.is_match("a/b"));
    }

    #[test]
    fn patterns_with_embedded_slashes_and_flag() {
        let r = parse_regex_literal("/a/b/i").unwrap();
        assert!(r.regex.is_match("A/B"));
    }

    #[test]
    fn escaped_forward_slashes_work() {
        let r = parse_regex_literal("/foo\\/bar/").unwrap();
        assert!(r.regex.is_match("foo/bar"));
    }

    #[test]
    fn backslash_d_works() {
        let r = parse_regex_literal("/\\d{3}/").unwrap();
        assert!(r.regex.is_match("abc123def"));
        assert!(!r.regex.is_match("abc12def"));
    }

    #[test]
    fn alternation_works() {
        let r = parse_regex_literal("/LGTM|SHIP IT/").unwrap();
        assert!(r.regex.is_match("LGTM"));
        assert!(r.regex.is_match("SHIP IT"));
        assert!(!r.regex.is_match("REJECT"));
    }

    // ── Error cases ──────────────────────────────────────────────────────────

    #[test]
    fn rejects_missing_leading_slash() {
        let err = parse_regex_literal("warn/").unwrap_err();
        assert!(err.contains("must start with '/'"));
    }

    #[test]
    fn rejects_g_flag_specifically() {
        let err = parse_regex_literal("/warn/g").unwrap_err();
        assert!(err.contains("'g' flag"));
        assert!(err.contains("not meaningful"));
    }

    #[test]
    fn rejects_gi_flags() {
        let err = parse_regex_literal("/warn/gi").unwrap_err();
        assert!(err.contains("'g' flag"));
    }

    #[test]
    fn rejects_x_flag() {
        let err = parse_regex_literal("/warn/x").unwrap_err();
        assert!(err.contains("unsupported"));
        assert!(err.contains("'x'"));
    }

    #[test]
    fn rejects_empty_pattern() {
        let err = parse_regex_literal("//").unwrap_err();
        assert!(err.contains("empty pattern"));
    }

    #[test]
    fn rejects_unterminated() {
        // `/foo` — no closing slash. No alphabetic tail to trigger the
        // unsupported-flags branch, so falls through to the unterminated path.
        let err = parse_regex_literal("/foo").unwrap_err();
        assert!(err.contains("not terminated") || err.contains("unterminated"));
    }

    #[test]
    fn rejects_invalid_regex_syntax() {
        // Unbalanced character class.
        let err = parse_regex_literal("/[unclosed/").unwrap_err();
        assert!(err.contains("failed to compile"));
    }

    #[test]
    fn preserves_source() {
        let r = parse_regex_literal("/warn|error/i").unwrap();
        assert_eq!(r.source, "/warn|error/i");
    }

    #[test]
    fn trims_outer_whitespace() {
        let r = parse_regex_literal("  /pass/i  ").unwrap();
        assert_eq!(r.source, "/pass/i");
    }
}
