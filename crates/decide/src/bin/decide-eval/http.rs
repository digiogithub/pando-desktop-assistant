//! Minimal blocking HTTP/1.1 transport over `std::net`, for the `decide-eval` dev tool
//! and the `#[ignore]` live tests only. Plain `http://` (a local Ollama) only; the app
//! ships its own `reqwest` transport with TLS. Kept out of the library so `decide`
//! itself has no network code.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use decide::transport::{DecisionTransport, TransportError, TransportRequest, TransportResponse};

/// `POST {base}/v1/systemone` over plain HTTP.
#[derive(Debug, Clone)]
pub struct StdHttpTransport {
    host: String,
    port: u16,
    path: String,
    api_key: Option<String>,
}

impl StdHttpTransport {
    /// `base` like `http://127.0.0.1:11434`. `api_key` is sent as a bearer token.
    pub fn new(base: &str, api_key: Option<String>) -> Result<Self, String> {
        let rest = base
            .strip_prefix("http://")
            .ok_or_else(|| format!("only http:// is supported, got {base:?}"))?;
        let (authority, prefix) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().map_err(|_| format!("bad port in {base:?}"))?),
            None => (authority, 80),
        };
        let prefix = prefix.trim_end_matches('/');
        let path = if prefix.is_empty() {
            "/v1/systemone".to_string()
        } else {
            format!("/{prefix}/v1/systemone")
        };
        Ok(Self {
            host: host.to_string(),
            port,
            path,
            api_key,
        })
    }

    fn redact(&self, s: String) -> String {
        match &self.api_key {
            Some(k) if !k.is_empty() => s.replace(k.as_str(), "***"),
            _ => s,
        }
    }

    fn post_blocking(&self, req: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let deadline = Instant::now() + req.timeout;
        let left = || {
            deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or(TransportError::Timeout)
        };
        let unreachable = |e: std::io::Error| {
            if matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                TransportError::Timeout
            } else {
                TransportError::Unreachable(self.redact(e.to_string()))
            }
        };
        let addrs: Vec<_> = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(unreachable)?
            .collect();
        let mut last = TransportError::Unreachable(format!("no address for {}", self.host));
        let mut stream = None;
        for a in addrs {
            match TcpStream::connect_timeout(&a, left()?) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last = unreachable(e),
            }
        }
        let mut stream = stream.ok_or(last)?;
        stream
            .set_write_timeout(Some(left()?))
            .map_err(unreachable)?;
        let mut head = format!(
            "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\n\
             Accept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.path,
            self.host,
            self.port,
            req.body.len()
        );
        if let Some(k) = self.api_key.as_deref().filter(|k| !k.is_empty()) {
            head.push_str(&format!("Authorization: Bearer {k}\r\n"));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).map_err(unreachable)?;
        stream.write_all(&req.body).map_err(unreachable)?;

        let mut raw = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            stream
                .set_read_timeout(Some(left()?))
                .map_err(unreachable)?;
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => raw.extend_from_slice(&buf[..n]),
                Err(e) => return Err(unreachable(e)),
            }
            if raw.len() > 4 << 20 {
                break;
            }
        }
        parse_response(&raw).map_err(TransportError::Unreachable)
    }
}

/// Parse a full HTTP/1.1 response (status, headers, identity or chunked body).
pub fn parse_response(raw: &[u8]) -> Result<TransportResponse, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("truncated HTTP response")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or("bad status line")?;
    let mut chunked = false;
    let mut length = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            } else if k == "content-length" {
                length = v.parse::<usize>().ok();
            }
        }
    }
    let rest = &raw[split + 4..];
    let body = if chunked {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let eol = rest[i..]
                .windows(2)
                .position(|w| w == b"\r\n")
                .ok_or("bad chunk")?;
            let size_line = String::from_utf8_lossy(&rest[i..i + eol]);
            let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| "bad chunk size")?;
            i += eol + 2;
            if size == 0 {
                break;
            }
            out.extend_from_slice(rest.get(i..i + size).ok_or("truncated chunk")?);
            i += size + 2;
        }
        out
    } else {
        match length {
            Some(n) => rest.get(..n).ok_or("truncated body")?.to_vec(),
            None => rest.to_vec(),
        }
    };
    Ok(TransportResponse { status, body })
}

impl DecisionTransport for StdHttpTransport {
    async fn post(&self, request: TransportRequest) -> Result<TransportResponse, TransportError> {
        self.post_blocking(&request)
    }
}

/// Default local Ollama.
#[allow(dead_code)]
pub const DEFAULT_BASE: &str = "http://127.0.0.1:11434";

/// A generous timeout for offline evaluation (cold model loads).
#[allow(dead_code)]
pub const EVAL_TIMEOUT: Duration = Duration::from_secs(30);
