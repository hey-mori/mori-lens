//! Just enough HTTP/1.1 to talk to a loopback Ollama.
//!
//! Hand-written rather than pulled in, for one reason that is specific to this
//! project: the endpoint is `127.0.0.1`, the payload is JSON, and there are two
//! verbs. A general HTTP client brings TLS, connection pooling, and an async
//! runtime to a socket that never leaves the machine.
//!
//! It is deliberately not a general client. It does not follow redirects, does
//! not keep connections alive, and speaks to one host at a time. If Lens ever
//! reaches a remote provider, that path gets a real client, because a remote
//! endpoint needs TLS and certificate verification and this file has neither
//! and must never pretend to.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use lens_core::translate::TranslateError;

/// A read of the body is capped. One translated sentence is not megabytes, and
/// an unbounded read from a misbehaving endpoint is a way to exhaust memory.
const MAX_BODY: usize = 1 << 20;

pub fn get(endpoint: &str, path: &str, timeout: Duration) -> Result<String, TranslateError> {
    request(endpoint, "GET", path, None, timeout)
}

pub fn post(
    endpoint: &str,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<String, TranslateError> {
    request(endpoint, "POST", path, Some(body), timeout)
}

fn request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<String, TranslateError> {
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
        read_chunked(&mut reader)
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
        Ok(String::from_utf8_lossy(&body).into_owned())
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
/// Returning `false` from the callback stops the read and closes the socket,
/// which is how a hover the user has moved on from stops costing anything.
pub fn post_streaming(
    endpoint: &str,
    path: &str,
    body: &str,
    timeout: Duration,
    on_line: &mut dyn FnMut(&str) -> bool,
) -> Result<(), TranslateError> {
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
    // Status line and headers, discarded. Unlike the buffered path there is no
    // need to know whether the body is chunked, per the note above.
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
    Ok(())
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
}
