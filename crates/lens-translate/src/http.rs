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
    let address = endpoint
        .to_socket_addrs()
        .map_err(|_| TranslateError::Unavailable)?
        .next()
        .ok_or(TranslateError::Unavailable)?;

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
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\n\
         Accept: application/json\r\n",
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
        let mut body = Vec::new();
        reader
            .take(MAX_BODY as u64)
            .read_to_end(&mut body)
            .map_err(|error| classify(&error))?;
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
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

    #[test]
    fn a_read_timeout_is_a_timeout_not_an_absent_engine() {
        // The distinction a user acts on: wait, versus start Ollama.
        let stalled = std::io::Error::new(std::io::ErrorKind::TimedOut, "stalled");
        assert_eq!(classify(&stalled), TranslateError::Timeout);

        let refused = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        assert_eq!(classify(&refused), TranslateError::Unavailable);
    }

    #[test]
    fn a_closed_port_is_unavailable_within_the_timeout() {
        let started = std::time::Instant::now();
        let result = get("127.0.0.1:1", "/api/tags", Duration::from_millis(300));
        assert_eq!(result, Err(TranslateError::Unavailable));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "a refused connection must not sit on the timeout",
        );
    }
}
