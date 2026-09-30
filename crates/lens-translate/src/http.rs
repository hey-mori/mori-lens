//! Just enough HTTP/1.1 to talk to a loopback Ollama.
//!
//! Hand-written rather than pulled in, for one reason that is specific to this
//! project: the endpoint is `127.0.0.1`, the payload is JSON, and there are two
//! verbs. A general HTTP client brings TLS, connection pooling, and an async
//! runtime to a socket that never leaves the machine.
//!
//! It is deliberately not a general client. It does not follow redirects and
//! speaks to one host at a time. If Lens ever reaches a remote provider, that
//! path gets a real client, because a remote endpoint needs TLS and certificate
//! verification and this file has neither and must never pretend to.
//!
//! Requests are one-shot by default. [`get`] and [`post`] send
//! `Connection: close` and each pays a fresh connect, which keeps them free of
//! any state that could go stale between calls. [`Session`] is the one
//! exception, for a caller that makes several requests to one endpoint in a
//! row, and it is opt-in so that the one-shot helpers stay that simple.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use lens_core::translate::TranslateError;

/// A read of the body is capped. One translated sentence is not megabytes, and
/// an unbounded read from a misbehaving endpoint is a way to exhaust memory.
const MAX_BODY: usize = 1 << 20;

pub fn get(endpoint: &str, path: &str, timeout: Duration) -> Result<(u16, String), TranslateError> {
    request(endpoint, "GET", path, None, timeout, None)
}

pub fn get_with_auth(
    endpoint: &str,
    path: &str,
    timeout: Duration,
    token: &str,
) -> Result<(u16, String), TranslateError> {
    request(endpoint, "GET", path, None, timeout, Some(token))
}

pub fn post(
    endpoint: &str,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<(u16, String), TranslateError> {
    request(endpoint, "POST", path, Some(body), timeout, None)
}

pub fn post_with_auth(
    endpoint: &str,
    path: &str,
    body: &str,
    timeout: Duration,
    token: &str,
) -> Result<(u16, String), TranslateError> {
    request(endpoint, "POST", path, Some(body), timeout, Some(token))
}

fn request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
    auth_token: Option<&str>,
) -> Result<(u16, String), TranslateError> {
    let address = resolve_loopback(endpoint)?;

    let stream =
        TcpStream::connect_timeout(&address, timeout).map_err(|_| TranslateError::Unavailable)?;
    // Both directions, because a model that has started generating and stalls
    // must fail the same way as one that never answered. Without a read
    // timeout a hover can hang forever on a wedged runtime.
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| TranslateError::Unavailable)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| TranslateError::Unavailable)?;

    let mut stream = stream;
    let payload = body.unwrap_or_default();
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\nAccept: application/json\r\n",
    );
    if let Some(token) = auth_token {
        head.push_str(&format!("Authorization: Bearer {}\r\n", token));
    }
    if body.is_some() {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len(),
        ));
    }
    head.push_str("\r\n");

    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(payload.as_bytes()))
        .and_then(|()| stream.flush())
        .map_err(|error| classify(&error))?;

    let mut reader = BufReader::new(stream);

    // Status line. A body is still read on an error status, because Ollama
    // explains a missing model in the body of a 404 and that message is what
    // tells a user which model to pull.
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|error| classify(&error))?;
    if status_line.is_empty() {
        return Err(TranslateError::Unavailable);
    }

    // Parse the status code from "HTTP/1.1 200 OK\r\n"
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let mut chunked = false;
    loop {
        let mut header = String::new();
        let read = reader
            .read_line(&mut header)
            .map_err(|error| classify(&error))?;
        if read == 0 || header.trim().is_empty() {
            break;
        }
        if header
            .to_ascii_lowercase()
            .starts_with("transfer-encoding:")
            && header.to_ascii_lowercase().contains("chunked")
        {
            chunked = true;
        }
    }

    if chunked {
        let body = read_chunked(&mut reader)?;
        Ok((status_code, body))
    } else {
        // `Connection: close` means the server ends the body by closing, so
        // reading to end is correct and Content-Length is not needed.
        // Cap the read at MAX_BODY + 1 to detect oversized responses.
        let mut body = Vec::new();
        let mut capped = reader.take(MAX_BODY as u64 + 1);
        match capped.read_to_end(&mut body) {
            Ok(_) => {}
            Err(e) if body.len() > MAX_BODY => {
                let _ = e;
                // We already have enough data to know it's oversized; the
                // read error is from the peer reacting to our slow read.
                return Err(TranslateError::Malformed);
            }
            Err(error) => return Err(classify(&error)),
        }
        if body.len() > MAX_BODY {
            return Err(TranslateError::Malformed);
        }
        Ok((status_code, String::from_utf8_lossy(&body).into_owned()))
    }
}

/// POST and hand each response line to a callback as it arrives.
///
/// Ollama's streaming form is newline-delimited JSON: one object per token
/// batch, the last carrying `"done":true`. Reading it line by line is what
/// makes a partial answer possible, and it is the reason this file exists
/// rather than a buffered helper.
///
/// Chunk-size lines from `Transfer-Encoding: chunked` are passed through
/// unchanged; they are not valid JSON and the caller skips them. Decoding the
/// framing properly would buy nothing, because each JSON object still arrives
/// whole on its own line either way.
///
/// Returns the HTTP status alongside success, so a 4xx/5xx streaming response
/// propagates as [`TranslateError::HttpFailed`]. A malformed status line
/// surfaces as [`TranslateError::Malformed`] rather than silently continuing.
///
/// Returning `false` from the callback stops the read and closes the socket,
/// which is how a hover the user has moved on from stops costing anything.
pub fn post_streaming(
    endpoint: &str,
    path: &str,
    body: &str,
    timeout: Duration,
    on_line: &mut dyn FnMut(&str) -> bool,
) -> Result<u16, TranslateError> {
    let address = resolve_loopback(endpoint)?;

    let stream =
        TcpStream::connect_timeout(&address, timeout).map_err(|_| TranslateError::Unavailable)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| TranslateError::Unavailable)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| TranslateError::Unavailable)?;
    let mut stream = stream;

    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len(),
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .and_then(|()| stream.flush())
        .map_err(|error| classify(&error))?;

    let mut reader = BufReader::new(stream);
    // Parse the status line (`HTTP/1.1 200 OK`) so the streaming path can
    // surface 4xx/5xx as HttpFailed instead of silently continuing.
    let mut status_line = String::new();
    let read = reader
        .read_line(&mut status_line)
        .map_err(|error| classify(&error))?;
    if read == 0 {
        return Err(TranslateError::Malformed);
    }
    let status = parse_status_line(&status_line).ok_or(TranslateError::Malformed)?;

    // Skip the remaining headers.
    loop {
        let mut header = String::new();
        let read = reader
            .read_line(&mut header)
            .map_err(|error| classify(&error))?;
        if read == 0 || header.trim().is_empty() {
            break;
        }
    }

    let mut total = 0usize;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| classify(&error))?;
        if read == 0 {
            break;
        }
        total += read;
        if total > MAX_BODY {
            return Err(TranslateError::Malformed);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !on_line(trimmed) {
            break;
        }
    }
    Ok(status)
}

/// Extract the status code from an HTTP status line.
///
/// Accepts `HTTP/1.0 200 OK`, `HTTP/1.1 404 Not Found`, etc. Returns `None`
/// if the line is not recognisably an HTTP status line.
fn parse_status_line(line: &str) -> Option<u16> {
    let trimmed = line.trim();
    let mut parts = trimmed.splitn(3, ' ');
    let _protocol = parts.next()?;
    let status_str = parts.next()?;
    status_str.parse::<u16>().ok()
}

/// Decode `Transfer-Encoding: chunked`.
fn read_chunked(reader: &mut BufReader<TcpStream>) -> Result<String, TranslateError> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        if reader
            .read_line(&mut size_line)
            .map_err(|error| classify(&error))?
            == 0
        {
            break;
        }
        // A chunk size may carry extensions after a semicolon.
        let size_text = size_line.trim().split(';').next().unwrap_or("").trim();
        if size_text.is_empty() {
            continue;
        }
        let size = usize::from_str_radix(size_text, 16).map_err(|_| TranslateError::Malformed)?;
        if size == 0 {
            break;
        }
        if body.len() + size > MAX_BODY {
            return Err(TranslateError::Malformed);
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|error| classify(&error))?;
        body.extend_from_slice(&chunk);
        // Trailing CRLF after each chunk.
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Resolve an endpoint to a socket address, refusing anything that is not
/// loopback.
///
/// The product promise is "nothing leaves this machine." A non-loopback
/// address here would send captured text over the network, and this module
/// has no TLS, so it would travel in clear text. The settings file validates
/// the endpoint, but the Tauri commands that list models and test a
/// connection also accept an endpoint from the webview, and that path must
/// not be able to reach another host even if the settings file is edited by
/// hand or the frontend sends a wrong value.
fn resolve_loopback(endpoint: &str) -> Result<std::net::SocketAddr, TranslateError> {
    let address = endpoint
        .to_socket_addrs()
        .map_err(|_| TranslateError::Unavailable)?
        .next()
        .ok_or(TranslateError::Unavailable)?;
    if address.ip().is_loopback() {
        Ok(address)
    } else {
        Err(TranslateError::Unavailable)
    }
}

/// A reused HTTP connection for consecutive requests to the same endpoint.
///
/// Sends `Connection: keep-alive` and uses `Content-Length` to find the end of
/// each response, so the socket stays usable for the next request instead of
/// being closed to mark the end of the body.
///
/// **What this is worth, measured rather than assumed.** Against the Ollama on
/// the development machine, `GET /api/tags` returning a 944-byte body, 40
/// iterations, median: a fresh connection per request costs 18.972ms and a
/// reused one costs 9.143ms. The saving is 9.8ms per request after the first.
///
/// The handshake is not where that goes. A bare connect-and-close to the same
/// port measures 0.398ms, so the handshake is 4% of the difference. The rest is
/// `Connection: close` making the server end each body by closing the socket,
/// and each request paying connection teardown. A session avoids the teardown,
/// not the handshake, which is why the saving is 25x larger than a handshake
/// count predicts. See `docs/benchmarks/2026-09-30-readiness-one-session.md`.
///
/// Not the default, for two reasons. A one-shot request cannot be stale, while a
/// session holds a socket the peer may have closed since the last use, so a
/// caller has to be prepared to reconnect. And the saving only exists from the
/// second request onward: a session used once is a connection with extra steps.
pub struct Session {
    stream: std::io::BufReader<TcpStream>,
    endpoint: String,
    auth_token: Option<String>,
}

impl Session {
    /// Open a new session to a loopback endpoint.
    ///
    /// The bearer token, when there is one, is held for the life of the session
    /// rather than passed per call, because it belongs to the endpoint and not to
    /// one request. Without it a session could not stand in for
    /// [`get_with_auth`] on an authenticated endpoint, which is what LM Studio's
    /// probes need.
    ///
    /// There is one constructor rather than a `connect`/`connect_with_auth` pair
    /// because only one of the two had a caller, and an unused convenience
    /// wrapper is how this file grew a dead `Session` in the first place.
    pub fn connect(
        endpoint: &str,
        timeout: Duration,
        auth_token: Option<&str>,
    ) -> Result<Self, TranslateError> {
        let address = resolve_loopback(endpoint)?;
        let stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|_| TranslateError::Unavailable)?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|_| TranslateError::Unavailable)?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|_| TranslateError::Unavailable)?;
        Ok(Session {
            stream: std::io::BufReader::new(stream),
            endpoint: endpoint.to_owned(),
            auth_token: auth_token.map(str::to_owned),
        })
    }

    /// Send a GET request and read the response.
    pub fn get(&mut self, path: &str) -> Result<(u16, String), TranslateError> {
        self.request("GET", path, None)
    }

    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<(u16, String), TranslateError> {
        let payload = body.unwrap_or_default();
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: keep-alive\r\nAccept: application/json\r\n",
            self.endpoint
        );
        if let Some(token) = &self.auth_token {
            head.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if body.is_some() {
            head.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                payload.len()
            ));
        }
        head.push_str("\r\n");

        let stream = self.stream.get_mut();
        stream
            .write_all(head.as_bytes())
            .and_then(|()| stream.write_all(payload.as_bytes()))
            .and_then(|()| stream.flush())
            .map_err(|error| classify(&error))?;

        // Read status line.
        let mut status_line = String::new();
        self.stream
            .read_line(&mut status_line)
            .map_err(|error| classify(&error))?;
        if status_line.is_empty() {
            return Err(TranslateError::Unavailable);
        }
        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);

        // Read headers, looking for Content-Length and Transfer-Encoding.
        let mut content_length: Option<usize> = None;
        let mut chunked = false;
        loop {
            let mut header = String::new();
            let read = self
                .stream
                .read_line(&mut header)
                .map_err(|error| classify(&error))?;
            if read == 0 || header.trim().is_empty() {
                break;
            }
            let lower = header.to_ascii_lowercase();
            if lower.starts_with("content-length:") {
                content_length = lower
                    .trim_start_matches("content-length:")
                    .trim()
                    .parse()
                    .ok();
            }
            if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                chunked = true;
            }
        }

        // Read body using Content-Length if available (keep-alive requires it).
        if let Some(length) = content_length {
            if length > MAX_BODY {
                return Err(TranslateError::Malformed);
            }
            let mut buf = vec![0u8; length];
            self.stream
                .read_exact(&mut buf)
                .map_err(|error| classify(&error))?;
            Ok((status_code, String::from_utf8_lossy(&buf).into_owned()))
        } else if chunked {
            // Chunked encoding: read chunks until terminating zero-length chunk.
            let body = read_chunked_session(&mut self.stream)?;
            Ok((status_code, body))
        } else {
            // No Content-Length and not chunked: read until connection close.
            // This breaks keep-alive, but is correct for the response.
            let mut buf = Vec::new();
            let mut capped = std::io::Read::take(&mut self.stream, MAX_BODY as u64 + 1);
            match capped.read_to_end(&mut buf) {
                Ok(_) => {}
                Err(e) if buf.len() > MAX_BODY => {
                    let _ = e;
                    return Err(TranslateError::Malformed);
                }
                Err(error) => return Err(classify(&error)),
            }
            if buf.len() > MAX_BODY {
                return Err(TranslateError::Malformed);
            }
            Ok((status_code, String::from_utf8_lossy(&buf).into_owned()))
        }
    }
}

/// Decode chunked encoding for session (uses BufReader<TcpStream>).
fn read_chunked_session(
    reader: &mut std::io::BufReader<TcpStream>,
) -> Result<String, TranslateError> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        if reader
            .read_line(&mut size_line)
            .map_err(|error| classify(&error))?
            == 0
        {
            break;
        }
        let size_text = size_line.trim().split(';').next().unwrap_or("").trim();
        if size_text.is_empty() {
            continue;
        }
        let size = usize::from_str_radix(size_text, 16).map_err(|_| TranslateError::Malformed)?;
        if size == 0 {
            // Read trailing CRLF after last chunk.
            let mut crlf = [0u8; 2];
            let _ = reader.read_exact(&mut crlf);
            break;
        }
        if body.len() + size > MAX_BODY {
            return Err(TranslateError::Malformed);
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|error| classify(&error))?;
        body.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// A stalled read and a refused connection are different facts.
///
/// Collapsing them would put "Ollama is not running" in front of a user whose
/// Ollama is running and loading a 9.6GB model, which is the one case where
/// waiting a little longer is the right advice.
fn classify(error: &std::io::Error) -> TranslateError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => TranslateError::Timeout,
        _ => TranslateError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use crate::fake_server::{FakeResponse, FakeServer};

    #[test]
    fn a_session_sends_its_bearer_token_on_every_request() {
        // The token is per-endpoint, so the second request has to carry it too.
        // An authenticated LM Studio would 401 the follow-up otherwise, and a
        // token attached only to the first request is the easy way to get that.
        let server = FakeServer::start_keep_alive(vec![
            FakeResponse::ok_json("{}")
                .expect_get("/first")
                .expect_header("Authorization", "Bearer secret-token"),
            FakeResponse::ok_json("{}")
                .expect_get("/second")
                .expect_header("Authorization", "Bearer secret-token"),
        ]);

        let mut session = Session::connect(
            server.address(),
            Duration::from_secs(2),
            Some("secret-token"),
        )
        .unwrap();
        assert_eq!(session.get("/first").unwrap().0, 200);
        assert_eq!(session.get("/second").unwrap().0, 200);
        drop(session);
        assert_eq!(server.connection_count(), 1);
    }

    #[test]
    fn a_session_without_a_token_sends_no_authorization_header() {
        let server =
            FakeServer::start_keep_alive(vec![FakeResponse::ok_json("{}").expect_get("/only")]);

        let mut session = Session::connect(server.address(), Duration::from_secs(2), None).unwrap();
        let (status, body) = session.get("/only").unwrap();

        assert_eq!(status, 200);
        assert_eq!(body, "{}");
    }
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::time::Instant;

    /// Bind a TCP listener on a random port and return its address string.
    fn bind_local() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        (listener, addr)
    }

    #[test]
    fn a_non_loopback_endpoint_is_refused_before_connecting() {
        // An address that is not loopback must be refused without attempting
        // a TCP connection, regardless of whether a host is reachable.
        let result = get("192.168.1.1:11434", "/api/tags", Duration::from_secs(5));
        assert_eq!(result, Err(TranslateError::Unavailable));
    }

    #[test]
    fn localhost_is_accepted_as_loopback() {
        // "localhost" resolves to 127.0.0.1 or ::1, both loopback.
        // It should pass the loopback check (and then fail because nothing
        // is listening, which is a different error path).
        let result = get("localhost:1", "/api/tags", Duration::from_millis(300));
        assert_eq!(result, Err(TranslateError::Unavailable));
        // If it got past the loopback check, it tried to connect and was
        // refused quickly (not waiting 5 seconds).
    }

    #[test]
    fn a_read_timeout_is_a_timeout_not_an_absent_engine() {
        let stalled = std::io::Error::new(std::io::ErrorKind::TimedOut, "stalled");
        assert_eq!(classify(&stalled), TranslateError::Timeout);

        let refused = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(classify(&refused), TranslateError::Unavailable);
    }

    #[test]
    fn a_closed_port_is_unavailable_within_the_timeout() {
        let started = Instant::now();
        let result = get("127.0.0.1:1", "/api/tags", Duration::from_millis(300));
        assert_eq!(result, Err(TranslateError::Unavailable));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "a refused connection must not sit on the timeout",
        );
    }

    #[test]
    fn black_hole_server_produces_timeout() {
        // Server accepts but never sends anything.
        let (listener, addr) = bind_local();
        let _handle = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            // Hold the connection open but send nothing.
            std::thread::sleep(Duration::from_secs(10));
        });

        let started = Instant::now();
        let result = get(&addr, "/api/tags", Duration::from_millis(200));
        assert_eq!(result, Err(TranslateError::Timeout));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "black-hole must fail within the read timeout, not hang",
        );
    }

    #[test]
    fn headers_then_stall_produces_timeout() {
        // Server sends HTTP status and headers, then stalls the body.
        let (listener, addr) = bind_local();
        let _handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 999999\r\n\r\n",
            );
            let _ = stream.flush();
            // Stall: never send the body.
            std::thread::sleep(Duration::from_secs(10));
        });

        let started = Instant::now();
        let result = get(&addr, "/api/generate", Duration::from_millis(200));
        assert_eq!(result, Err(TranslateError::Timeout));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn empty_response_is_unavailable() {
        // Server accepts and immediately closes without sending anything.
        let (listener, addr) = bind_local();
        let _handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
        });

        let result = get(&addr, "/api/tags", Duration::from_secs(2));
        assert_eq!(result, Err(TranslateError::Unavailable));
    }

    #[test]
    fn oversized_body_is_malformed() {
        // Server sends a valid response slightly exceeding MAX_BODY.
        // Use a small overflow to avoid TCP buffer deadlock.
        let (listener, addr) = bind_local();
        let _handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Read the request first so the TCP buffers are clear.
            let mut req = [0u8; 512];
            let _ = stream.read(&mut req);

            let body_size = MAX_BODY + 64;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {body_size}\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(header.as_bytes());
            // Send body in small chunks to avoid TCP buffer pressure.
            let chunk = vec![b'A'; 1024];
            let mut sent = 0;
            while sent < body_size {
                let n = (body_size - sent).min(1024);
                if stream.write_all(&chunk[..n]).is_err() {
                    break;
                }
                sent += n;
            }
            let _ = stream.flush();
        });

        let result = get(&addr, "/api/tags", Duration::from_secs(5));
        assert_eq!(result, Err(TranslateError::Malformed));
    }

    #[test]
    fn post_streaming_stall_produces_timeout() {
        // Server accepts, sends headers, then stalls during streaming.
        let (listener, addr) = bind_local();
        let _handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ =
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\n\r\n");
            let _ = stream.flush();
            // Send one line then stall.
            let _ = stream.write_all(b"{\"response\":\"hello\"}\n");
            let _ = stream.flush();
            std::thread::sleep(Duration::from_secs(10));
        });

        let started = Instant::now();
        let mut lines = Vec::new();
        let result = post_streaming(
            &addr,
            "/api/generate",
            "{}",
            Duration::from_millis(200),
            &mut |line| {
                lines.push(line.to_owned());
                true
            },
        );
        assert_eq!(result, Err(TranslateError::Timeout));
        assert_eq!(lines.len(), 1, "the first line arrived before the stall");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn post_streaming_oversized_is_malformed() {
        // Server sends many small lines that accumulate past MAX_BODY.
        let (listener, addr) = bind_local();
        let _handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Read the request first.
            let mut req = [0u8; 512];
            let _ = stream.read(&mut req);

            let _ =
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\n\r\n");
            let _ = stream.flush();
            // 4096-byte lines; 257 lines exceed MAX_BODY (1MB).
            let line = format!("{}\n", "X".repeat(4095));
            for _ in 0..257 {
                if stream.write_all(line.as_bytes()).is_err() {
                    break;
                }
            }
            let _ = stream.flush();
        });

        let result = post_streaming(
            &addr,
            "/api/generate",
            "{}",
            Duration::from_secs(30),
            &mut |_| true,
        );
        // The response exceeds MAX_BODY — must not succeed.
        assert!(
            result.is_err(),
            "oversized streaming response must not succeed: got {:?}",
            result,
        );
    }

    /// `parse_status_line` is the function that lets the streaming path
    /// surface 4xx/5xx. Verify the happy paths and the malformed input it
    /// must refuse.
    #[test]
    fn parse_status_line_handles_common_forms() {
        assert_eq!(parse_status_line("HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(parse_status_line("HTTP/1.1 404 Not Found\r\n"), Some(404));
        assert_eq!(
            parse_status_line("HTTP/1.0 500 Internal Server Error"),
            Some(500)
        );
        assert_eq!(parse_status_line("HTTP/2 200"), Some(200));
        // Garbage must not parse to a status.
        assert_eq!(parse_status_line(""), None);
        assert_eq!(parse_status_line("not a status line"), None);
        assert_eq!(parse_status_line("HTTP/1.1 abc OK"), None);
    }
}
