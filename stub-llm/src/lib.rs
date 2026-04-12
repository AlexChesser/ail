//! A lightweight stub HTTP server for LLM integration testing.
//!
//! `StubLlmServer` binds to `127.0.0.1:0` (OS-assigned port), serves pre-recorded
//! responses, and shuts down cleanly on `Drop`. Each test gets its own server on a
//! unique port, enabling full parallelism under `cargo nextest`.
//!
//! # Quick Start
//!
//! ```rust
//! use stub_llm::{StubLlmServer, StubResponse};
//!
//! let server = StubLlmServer::new(vec![
//!     StubResponse::Success {
//!         content: "Hello from stub!".to_string(),
//!         model: None,
//!         usage: None,
//!     },
//! ]);
//!
//! // Point your HTTP-based LLM client at server.base_url()
//! let url = server.base_url();
//! assert!(url.starts_with("http://127.0.0.1:"));
//!
//! // After the test, inspect what was sent:
//! // let requests = server.requests();
//! ```

mod http_parse;
mod openai;

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use http_parse::{format_response, parse_request};
use openai::build_success_response;

// ── Public types ─────────────────────────────────────────────────────────────

/// A pre-recorded HTTP response for the stub server to return.
#[derive(Debug, Clone)]
pub enum StubResponse {
    /// A valid OpenAI-compatible chat completion response.
    Success {
        /// The assistant's reply text.
        content: String,
        /// Model name in the response. Defaults to `"stub-model"`.
        model: Option<String>,
        /// `(prompt_tokens, completion_tokens)`. Defaults to `(0, 0)`.
        usage: Option<(u64, u64)>,
    },
    /// A raw HTTP response for testing error paths.
    Raw {
        /// HTTP status code (e.g. 500).
        status_code: u16,
        /// Raw response body.
        body: String,
    },
}

/// A recorded incoming HTTP request.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    /// HTTP method (e.g. `"POST"`).
    pub method: String,
    /// Request path (e.g. `"/v1/chat/completions"`).
    pub path: String,
    /// All request headers as `(name, value)` pairs.
    pub headers: Vec<(String, String)>,
    /// The request body (typically JSON).
    pub body: String,
}

// ── Server ───────────────────────────────────────────────────────────────────

/// A lightweight HTTP server returning pre-recorded LLM responses.
///
/// Binds to `127.0.0.1:0` (OS-assigned port) for parallel test safety.
/// Shuts down cleanly on [`Drop`] via a dummy-connect sentinel.
pub struct StubLlmServer {
    port: u16,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

impl StubLlmServer {
    /// Start a server that returns the given responses in order.
    ///
    /// After all responses are consumed, subsequent requests receive the last
    /// response in the list. If the list is empty, all requests get HTTP 500.
    pub fn new(responses: Vec<StubResponse>) -> Self {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("stub-llm: failed to bind to localhost:0");
        let port = listener.local_addr().unwrap().port();

        let shutdown = Arc::new(AtomicBool::new(false));
        let requests: Arc<Mutex<Vec<RecordedRequest>>> = Arc::new(Mutex::new(Vec::new()));

        let shutdown_flag = Arc::clone(&shutdown);
        let requests_ref = Arc::clone(&requests);

        let handle = thread::spawn(move || {
            accept_loop(listener, shutdown_flag, requests_ref, responses);
        });

        StubLlmServer {
            port,
            shutdown,
            handle: Some(handle),
            requests,
        }
    }

    /// The base URL to pass to an HTTP runner config.
    ///
    /// Returns a URL like `"http://127.0.0.1:54321/v1"` — the `/v1` prefix is
    /// included so the runner appends `/chat/completions` to form the full path.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// Returns a clone of all recorded requests received so far.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl Drop for StubLlmServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // Dummy-connect to unblock the accept() call, same pattern as
        // ClaudePermissionListener in ail-core.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

// ── Accept loop ──────────────────────────────────────────────────────────────

fn accept_loop(
    listener: TcpListener,
    shutdown: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    responses: Vec<StubResponse>,
) {
    let mut response_index: usize = 0;

    for stream in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Parse the incoming request
        let parsed = match parse_request(&stream) {
            Ok(Some(req)) => req,
            Ok(None) => continue, // Empty connection (shutdown sentinel)
            Err(_) => continue,
        };

        // Record the request
        {
            let mut reqs = requests.lock().expect("requests lock");
            reqs.push(RecordedRequest {
                method: parsed.method.clone(),
                path: parsed.path.clone(),
                headers: parsed.headers.clone(),
                body: parsed.body.clone(),
            });
        }

        // Pick the next response
        let response_bytes = if responses.is_empty() {
            format_response(
                500,
                "Internal Server Error",
                r#"{"error":"no responses configured"}"#,
            )
        } else {
            let idx = if response_index < responses.len() {
                let i = response_index;
                response_index += 1;
                i
            } else {
                responses.len() - 1 // Repeat last
            };
            build_http_response(&responses[idx])
        };

        // Send the response — ignore write errors (client may have disconnected)
        let _ = (&stream).write_all(&response_bytes);
        let _ = (&stream).flush();
    }
}

fn build_http_response(response: &StubResponse) -> Vec<u8> {
    match response {
        StubResponse::Success {
            content,
            model,
            usage,
        } => {
            let json = build_success_response(content, model.as_deref(), *usage);
            let body = serde_json::to_string(&json).unwrap();
            format_response(200, "OK", &body)
        }
        StubResponse::Raw { status_code, body } => {
            let status_text = match *status_code {
                200 => "OK",
                400 => "Bad Request",
                401 => "Unauthorized",
                403 => "Forbidden",
                404 => "Not Found",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                502 => "Bad Gateway",
                503 => "Service Unavailable",
                _ => "Error",
            };
            format_response(*status_code, status_text, body)
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn server_starts_and_returns_base_url() {
        let server = StubLlmServer::new(vec![StubResponse::Success {
            content: "hello".to_string(),
            model: None,
            usage: None,
        }]);
        let url = server.base_url();
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(url.ends_with("/v1"));
    }

    #[test]
    fn server_returns_canned_response() {
        let server = StubLlmServer::new(vec![StubResponse::Success {
            content: "test response".to_string(),
            model: Some("test-model".to_string()),
            usage: Some((10, 5)),
        }]);

        // Make a manual HTTP request
        let port = server.port;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let req_body = r#"{"model":"test","messages":[]}"#;
        write!(
            stream,
            "POST /v1/chat/completions HTTP/1.1\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            req_body.len(),
            req_body
        )
        .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.contains("200 OK"));
        assert!(response.contains("test response"));
        assert!(response.contains("test-model"));
    }

    #[test]
    fn server_records_requests() {
        let server = StubLlmServer::new(vec![StubResponse::Success {
            content: "ok".to_string(),
            model: None,
            usage: None,
        }]);

        let port = server.port;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = r#"{"model":"gpt-4"}"#;
        write!(
            stream,
            "POST /v1/chat/completions HTTP/1.1\r\n\
             Authorization: Bearer secret\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            body.len(),
            body
        )
        .unwrap();
        stream.flush().unwrap();

        // Read response to completion
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].path, "/v1/chat/completions");
        assert_eq!(requests[0].body, r#"{"model":"gpt-4"}"#);
        assert!(requests[0]
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v == "Bearer secret"));
    }

    #[test]
    fn server_returns_raw_error_response() {
        let server = StubLlmServer::new(vec![StubResponse::Raw {
            status_code: 500,
            body: "internal error".to_string(),
        }]);

        let port = server.port;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "POST /v1/chat/completions HTTP/1.1\r\n\
             Content-Length: 2\r\n\
             \r\n\
             {{}}"
        )
        .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.contains("500 Internal Server Error"));
        assert!(response.contains("internal error"));
    }

    #[test]
    fn server_repeats_last_response_when_exhausted() {
        let server = StubLlmServer::new(vec![StubResponse::Success {
            content: "only-one".to_string(),
            model: None,
            usage: None,
        }]);

        let port = server.port;

        // First request
        {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
            let mut r = String::new();
            s.read_to_string(&mut r).unwrap();
            assert!(r.contains("only-one"));
        }

        // Second request — should repeat last
        {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
            let mut r = String::new();
            s.read_to_string(&mut r).unwrap();
            assert!(r.contains("only-one"));
        }

        assert_eq!(server.requests().len(), 2);
    }

    #[test]
    fn server_sequences_multiple_responses() {
        let server = StubLlmServer::new(vec![
            StubResponse::Success {
                content: "first".to_string(),
                model: None,
                usage: None,
            },
            StubResponse::Success {
                content: "second".to_string(),
                model: None,
                usage: None,
            },
        ]);

        let port = server.port;

        let mut r1 = String::new();
        {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
            s.read_to_string(&mut r1).unwrap();
        }

        let mut r2 = String::new();
        {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
            s.read_to_string(&mut r2).unwrap();
        }

        assert!(r1.contains("first"));
        assert!(r2.contains("second"));
    }

    #[test]
    fn server_shuts_down_on_drop() {
        let port;
        {
            let server = StubLlmServer::new(vec![]);
            port = server.port;
            // Server is dropped here
        }
        // After drop, connection should be refused
        let result = TcpStream::connect(("127.0.0.1", port));
        assert!(result.is_err(), "Server should be shut down");
    }
}
