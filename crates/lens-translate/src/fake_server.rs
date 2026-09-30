//! Deterministic fake HTTP server for provider contract tests.
//!
//! This module provides a test harness for validating provider runtime
//! implementations without requiring real Ollama or LM Studio installations.
//! All tests using this harness are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.
//!
//! The harness verifies that outgoing requests match expectations (method, path,
//! headers, body). A contract test that sends the wrong path or omits an
//! authentication header fails rather than silently returning a queued response.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

/// Expected properties of an incoming HTTP request.
///
/// Each field is optional. When `None`, any value is accepted. When `Some`,
/// the received request must match exactly or the test panics.
#[derive(Debug, Clone, Default)]
pub struct RequestExpectation {
    /// Expected HTTP method (e.g., "GET", "POST").
    pub method: Option<String>,
    /// Expected request path (e.g., "/api/v1/models").
    pub path: Option<String>,
    /// Expected headers (name, value). Both are case-insensitive for names.
    pub headers: Vec<(String, String)>,
    /// Expected body (for POST requests).
    pub body: Option<String>,
}

/// Configuration for a fake server response.
#[derive(Debug, Clone)]
pub struct FakeResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// If set, the server verifies the incoming request matches these expectations.
    pub expect: Option<RequestExpectation>,
}

impl FakeResponse {
    pub fn ok_json(body: &str) -> Self {
        Self {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.to_owned(),
            expect: None,
        }
    }

    /// Create a response with a custom status code.
    ///
    /// Used for tests that need specific status codes not covered by the
    /// convenience constructors (e.g., 400, 500).
    pub fn status(code: u16) -> Self {
        let text = match code {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        };
        Self {
            status: code,
            status_text: text.to_owned(),
            headers: vec![],
            body: String::new(),
            expect: None,
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            status_text: "Not Found".to_owned(),
            headers: vec![],
            body: String::new(),
            expect: None,
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            status_text: "Unauthorized".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: r#"{"error":{"message":"Invalid or missing API token","type":"authentication_error"}}"#.to_owned(),
            expect: None,
        }
    }

    pub fn forbidden() -> Self {
        Self {
            status: 403,
            status_text: "Forbidden".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: r#"{"error":{"message":"Insufficient permissions","type":"permission_error"}}"#
                .to_owned(),
            expect: None,
        }
    }

    pub fn bad_request() -> Self {
        Self {
            status: 400,
            status_text: "Bad Request".to_owned(),
            headers: vec![],
            body: String::new(),
            expect: None,
        }
    }

    pub fn server_error() -> Self {
        Self {
            status: 500,
            status_text: "Internal Server Error".to_owned(),
            headers: vec![],
            body: String::new(),
            expect: None,
        }
    }

    /// Create a 404 response with a custom JSON body.
    ///
    /// Used for model-not-found tests where the body contains error details.
    pub fn not_found_with_body(body: &str) -> Self {
        Self {
            status: 404,
            status_text: "Not Found".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.to_owned(),
            expect: None,
        }
    }

    /// Set the expectation that the request is a GET to the given path.
    pub fn expect_get(mut self, path: &str) -> Self {
        self.expect = Some(RequestExpectation {
            method: Some("GET".to_owned()),
            path: Some(path.to_owned()),
            ..Default::default()
        });
        self
    }

    /// Set the expectation that the request is a POST to the given path.
    pub fn expect_post(mut self, path: &str) -> Self {
        self.expect = Some(RequestExpectation {
            method: Some("POST".to_owned()),
            path: Some(path.to_owned()),
            ..Default::default()
        });
        self
    }

    /// Add an expected header to the current expectation.
    ///
    /// Creates a default expectation if none exists yet.
    pub fn expect_header(mut self, name: &str, value: &str) -> Self {
        let expect = self.expect.get_or_insert_with(RequestExpectation::default);
        expect.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Set the expected body for the current expectation.
    ///
    /// Creates a default expectation if none exists yet.
    #[allow(dead_code)] // Used by translator contract tests
    pub fn expect_body(mut self, body: &str) -> Self {
        let expect = self.expect.get_or_insert_with(RequestExpectation::default);
        expect.body = Some(body.to_owned());
        self
    }
}

/// A parsed HTTP request received by the fake server.
struct ReceivedRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// Verify that a received request matches the expectation.
///
/// Panics with a descriptive message if any expectation is not met.
fn verify_request(received: &ReceivedRequest, expect: &RequestExpectation) {
    if let Some(expected_method) = &expect.method {
        assert_eq!(
            received.method, *expected_method,
            "contract violation: expected method {}, got {}",
            expected_method, received.method
        );
    }

    if let Some(expected_path) = &expect.path {
        assert_eq!(
            received.path, *expected_path,
            "contract violation: expected path {}, got {}",
            expected_path, received.path
        );
    }

    for (expected_name, expected_value) in &expect.headers {
        let found = received.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case(expected_name) && value == expected_value
        });
        assert!(
            found,
            "contract violation: expected header {}: {} not found in request",
            expected_name, expected_value
        );
    }

    if let Some(expected_body) = &expect.body {
        assert_eq!(
            received.body, *expected_body,
            "contract violation: request body mismatch.\nexpected: {}\ngot: {}",
            expected_body, received.body
        );
    }
}

/// A fake HTTP server that handles multiple sequential requests.
///
/// Responses are queued before starting the server. Each request consumes
/// one response from the queue. If the response has an expectation, the
/// server verifies the incoming request matches before responding.
/// The server thread exits after handling all queued responses.
pub struct FakeServer {
    address: String,
    handle: Option<thread::JoinHandle<()>>,
    /// How many TCP connections the server has accepted.
    ///
    /// A test that claims a connection was reused has to be able to count them,
    /// otherwise it asserts on the responses it queued rather than on the socket
    /// behaviour it cares about.
    connections: Arc<AtomicUsize>,
}

impl FakeServer {
    /// Start a fake server with queued responses.
    ///
    /// The server will handle up to `responses.len()` requests, returning
    /// each response in order. After all responses are consumed, the server
    /// thread exits naturally. If a response has an expectation set, the
    /// server panics if the incoming request does not match.
    pub fn start(responses: Vec<FakeResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let address = listener.local_addr().unwrap().to_string();
        let num_responses = responses.len();
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connections);

        let handle = thread::spawn(move || {
            let mut response_queue = responses;

            for _ in 0..num_responses {
                let mut stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(_) => break,
                };
                counter.fetch_add(1, Ordering::SeqCst);

                if response_queue.is_empty() {
                    break;
                }

                let response = response_queue.remove(0);

                // Read request
                let mut reader = BufReader::new(match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => break,
                });

                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    break;
                }
                let parts: Vec<&str> = request_line.split_whitespace().collect();
                if parts.len() < 2 {
                    break;
                }

                let method = parts[0].to_owned();
                let path = parts[1].to_owned();

                // Read headers
                let mut received_headers = Vec::new();
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some((key, value)) = trimmed.split_once(':') {
                        let key = key.trim().to_owned();
                        let value = value.trim().to_owned();
                        if key.eq_ignore_ascii_case("content-length") {
                            content_length = value.parse().unwrap_or(0);
                        }
                        received_headers.push((key, value));
                    }
                }

                // Read body
                let mut body = String::new();
                if content_length > 0 {
                    let mut buf = vec![0u8; content_length];
                    if reader.read_exact(&mut buf).is_ok() {
                        body = String::from_utf8_lossy(&buf).into_owned();
                    }
                }

                // Verify request against expectation
                if let Some(expectation) = &response.expect {
                    let received = ReceivedRequest {
                        method,
                        path,
                        headers: received_headers,
                        body,
                    };
                    verify_request(&received, expectation);
                }

                // Send response
                let status_line =
                    format!("HTTP/1.1 {} {}\r\n", response.status, response.status_text);
                let _ = stream.write_all(status_line.as_bytes());

                for (key, value) in &response.headers {
                    let header = format!("{}: {}\r\n", key, value);
                    let _ = stream.write_all(header.as_bytes());
                }

                let content_length_header = format!("Content-Length: {}\r\n", response.body.len());
                let _ = stream.write_all(content_length_header.as_bytes());
                let _ = stream.write_all(b"Connection: close\r\n");
                let _ = stream.write_all(b"\r\n");
                let _ = stream.write_all(response.body.as_bytes());
                let _ = stream.flush();
            }
        });

        Self {
            address,
            handle: Some(handle),
            connections,
        }
    }

    /// Start a fake server that serves every queued response on one connection.
    ///
    /// Replies `Connection: keep-alive` with a `Content-Length`, so a client
    /// finds the end of each body without the socket closing and can send the
    /// next request down the same one. [`Self::start`] closes after every
    /// response, which makes it unable to observe reuse at all: to it, a client
    /// that reuses a connection and one that does not look identical.
    ///
    /// The thread stops when the queue empties or the client goes away, so a
    /// client that sends fewer requests than there are queued responses does not
    /// hang the test in `drop`.
    pub fn start_keep_alive(responses: Vec<FakeResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let address = listener.local_addr().unwrap().to_string();
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connections);

        let handle = thread::spawn(move || {
            let mut response_queue = responses;
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let Ok(mut writer) = stream.try_clone() else {
                return;
            };
            let mut reader = BufReader::new(stream);

            while !response_queue.is_empty() {
                let mut request_line = String::new();
                match reader.read_line(&mut request_line) {
                    // EOF: the client is finished and will not ask again.
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                let parts: Vec<&str> = request_line.split_whitespace().collect();
                if parts.len() < 2 {
                    break;
                }
                let method = parts[0].to_owned();
                let path = parts[1].to_owned();

                let mut received_headers = Vec::new();
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        break;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some((key, value)) = trimmed.split_once(':') {
                        let key = key.trim().to_owned();
                        let value = value.trim().to_owned();
                        if key.eq_ignore_ascii_case("content-length") {
                            content_length = value.parse().unwrap_or(0);
                        }
                        received_headers.push((key, value));
                    }
                }

                let mut body = String::new();
                if content_length > 0 {
                    let mut buf = vec![0u8; content_length];
                    if reader.read_exact(&mut buf).is_ok() {
                        body = String::from_utf8_lossy(&buf).into_owned();
                    }
                }

                let response = response_queue.remove(0);
                if let Some(expectation) = &response.expect {
                    let received = ReceivedRequest {
                        method,
                        path,
                        headers: received_headers,
                        body,
                    };
                    verify_request(&received, expectation);
                }

                let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, response.status_text);
                for (key, value) in &response.headers {
                    out.push_str(&format!("{key}: {value}\r\n"));
                }
                out.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
                out.push_str("Connection: keep-alive\r\n\r\n");
                out.push_str(&response.body);
                let _ = writer.write_all(out.as_bytes());
                let _ = writer.flush();
            }
        });

        Self {
            address,
            handle: Some(handle),
            connections,
        }
    }

    /// Get the server address (e.g., "127.0.0.1:12345").
    pub fn address(&self) -> &str {
        &self.address
    }

    /// How many TCP connections this server has accepted.
    pub fn connection_count(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            if let Err(panic_payload) = handle.join() {
                // Propagate contract violations to the test thread so
                // #[should_panic] can catch them.
                std::panic::resume_unwind(panic_payload);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn fake_server_handles_get_request() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"test":"value"}"#)]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        stream
            .write_all(b"GET /api/test HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.contains(r#"{"test":"value"}"#));
        assert!(response.contains("200 OK"));
    }

    #[test]
    fn fake_server_handles_multiple_requests() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"first":1}"#),
            FakeResponse::ok_json(r#"{"second":2}"#),
        ]);

        // First request
        let mut stream1 = TcpStream::connect(server.address()).unwrap();
        stream1
            .write_all(b"GET /first HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream1.flush().unwrap();
        let mut response1 = String::new();
        stream1.read_to_string(&mut response1).unwrap();
        assert!(response1.contains(r#"{"first":1}"#));

        // Second request
        let mut stream2 = TcpStream::connect(server.address()).unwrap();
        stream2
            .write_all(b"GET /second HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream2.flush().unwrap();
        let mut response2 = String::new();
        stream2.read_to_string(&mut response2).unwrap();
        assert!(response2.contains(r#"{"second":2}"#));
    }

    #[test]
    fn fake_server_handles_post_with_body() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"received":true}"#)]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        stream
            .write_all(
                b"POST /api/generate HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"model\":\"test\"}",
            )
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.contains(r#"{"received":true}"#));
    }

    #[test]
    fn fake_server_returns_error_status() {
        let server = FakeServer::start(vec![FakeResponse::not_found()]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        stream
            .write_all(b"GET /missing HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("404"));
    }

    #[test]
    fn fake_server_verifies_expected_method_and_path() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/tags")
        ]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        stream
            .write_all(b"GET /api/tags HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains(r#"{"models":[]}"#));
    }

    #[test]
    fn fake_server_verifies_expected_header() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"data":[]}"#)
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer test-token")]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        stream
            .write_all(
                b"GET /api/v1/models HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\n\r\n",
            )
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains(r#"{"data":[]}"#));
    }

    #[test]
    #[should_panic(expected = "contract violation: expected path")]
    fn fake_server_panics_on_wrong_path() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/v1/models")
        ]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        // Wrong path: /v1/models instead of /api/v1/models
        stream
            .write_all(b"GET /v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
    }

    #[test]
    #[should_panic(expected = "contract violation: expected method")]
    fn fake_server_panics_on_wrong_method() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/v1/models")
        ]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        // Wrong method: POST instead of GET
        stream
            .write_all(
                b"POST /api/v1/models HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
    }

    #[test]
    #[should_panic(expected = "contract violation: expected header")]
    fn fake_server_panics_on_missing_header() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"data":[]}"#)
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer test-token")]);

        let mut stream = TcpStream::connect(server.address()).unwrap();
        // Missing Authorization header
        stream
            .write_all(b"GET /api/v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();

        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
    }
}
