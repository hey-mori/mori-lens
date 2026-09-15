//! Deterministic fake HTTP server for provider contract tests.
//!
//! This module provides a test harness for validating provider runtime
//! implementations without requiring real Ollama or LM Studio installations.
//! All tests using this harness are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

/// Configuration for a fake server response.
#[derive(Debug, Clone)]
pub struct FakeResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl FakeResponse {
    pub fn ok_json(body: &str) -> Self {
        Self {
            status: 200,
            status_text: "OK".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.to_owned(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            status_text: "Not Found".to_owned(),
            headers: vec![],
            body: String::new(),
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            status: 401,
            status_text: "Unauthorized".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: r#"{"error":{"message":"Invalid or missing API token","type":"authentication_error"}}"#.to_owned(),
        }
    }

    pub fn forbidden() -> Self {
        Self {
            status: 403,
            status_text: "Forbidden".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: r#"{"error":{"message":"Insufficient permissions","type":"permission_error"}}"#
                .to_owned(),
        }
    }

    #[allow(dead_code)] // Available for future contract tests
    pub fn server_error() -> Self {
        Self {
            status: 500,
            status_text: "Internal Server Error".to_owned(),
            headers: vec![],
            body: String::new(),
        }
    }
}

/// A fake HTTP server that handles multiple sequential requests.
///
/// Responses are queued before starting the server. Each request consumes
/// one response from the queue. The server thread exits after handling all
/// queued responses or when the listener is dropped.
pub struct FakeServer {
    address: String,
    handle: Option<thread::JoinHandle<()>>,
}

impl FakeServer {
    /// Start a fake server with queued responses.
    ///
    /// The server will handle up to `responses.len()` requests, returning
    /// each response in order. After all responses are consumed, the server
    /// thread exits naturally.
    pub fn start(responses: Vec<FakeResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let address = listener.local_addr().unwrap().to_string();
        let num_responses = responses.len();

        let handle = thread::spawn(move || {
            let mut response_queue = responses;

            // Accept exactly num_responses connections, then exit
            for _ in 0..num_responses {
                let mut stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(_) => break,
                };

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

                // Read headers
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
                        if key.trim().to_lowercase() == "content-length" {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                    }
                }

                // Read body
                if content_length > 0 {
                    let mut buf = vec![0u8; content_length];
                    let _ = reader.read_exact(&mut buf);
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
            // Listener is dropped here, closing the port
        });

        Self {
            address,
            handle: Some(handle),
        }
    }

    /// Get the server address (e.g., "127.0.0.1:12345").
    pub fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
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
                b"POST /api/generate HTTP/1.1\r\n\
                  Host: localhost\r\n\
                  Content-Type: application/json\r\n\
                  Content-Length: 13\r\n\
                  \r\n\
                  {\"model\":\"test\"}",
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
}
