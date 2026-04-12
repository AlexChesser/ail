//! Minimal HTTP/1.1 request parser for the stub LLM server.
//!
//! Only parses enough to extract the method, path, headers, and body from a
//! standard HTTP/1.1 request with `Content-Length`. This is sufficient for
//! parsing requests from `ureq` and similar synchronous HTTP clients.

use std::io::{self, BufRead, BufReader, Read};
use std::net::TcpStream;

/// A parsed HTTP request.
#[derive(Debug)]
pub(crate) struct ParsedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Read and parse one HTTP/1.1 request from a TCP stream.
///
/// Returns `None` if the connection was closed before a complete request
/// could be read (e.g. the dummy-connect used for shutdown).
pub(crate) fn parse_request(stream: &TcpStream) -> io::Result<Option<ParsedRequest>> {
    let mut reader = BufReader::new(stream);

    // ── Request line ────────────────────────────────────────────────────
    let mut request_line = String::new();
    let n = reader.read_line(&mut request_line)?;
    if n == 0 {
        return Ok(None); // Connection closed immediately (shutdown sentinel)
    }

    let mut parts = request_line.trim().splitn(3, ' ');
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    // HTTP version ignored

    if method.is_empty() {
        return Ok(None);
    }

    // ── Headers ─────────────────────────────────────────────────────────
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length: usize = 0;

    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // End of headers
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }

    // ── Body ────────────────────────────────────────────────────────────
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    Ok(Some(ParsedRequest {
        method,
        path,
        headers,
        body,
    }))
}

/// Format a minimal HTTP/1.1 response.
pub(crate) fn format_response(status_code: u16, status_text: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status_code} {status_text}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn parse_simple_post_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let body = r#"{"model":"test"}"#;
        let handle = std::thread::spawn(move || {
            let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                client,
                "POST /v1/chat/completions HTTP/1.1\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Authorization: Bearer tok123\r\n\
                 \r\n\
                 {}",
                body.len(),
                body
            )
            .unwrap();
        });

        let (stream, _) = listener.accept().unwrap();
        let req = parse_request(&stream).unwrap().unwrap();

        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/chat/completions");
        assert_eq!(req.body, r#"{"model":"test"}"#);
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v == "Bearer tok123"));

        handle.join().unwrap();
    }

    #[test]
    fn parse_empty_connection_returns_none() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let _client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            // Drop immediately — no data sent
        });

        let (stream, _) = listener.accept().unwrap();
        let result = parse_request(&stream).unwrap();
        assert!(result.is_none());

        handle.join().unwrap();
    }

    #[test]
    fn format_response_has_correct_structure() {
        let resp = format_response(200, "OK", r#"{"ok":true}"#);
        let text = String::from_utf8(resp).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Length: 11\r\n"));
        assert!(text.contains(r#"{"ok":true}"#));
    }
}
