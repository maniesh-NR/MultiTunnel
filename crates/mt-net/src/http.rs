//! HTTP proxy server (inbound): absolute-form requests + CONNECT tunneling.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"): HTTP proxy server with
//! absolute-form requests and CONNECT tunneling; 502 on upstream failure;
//! bounded header sizes. The request/tunnel flow lives in `server.rs`; this
//! module owns head reading, parsing, rewriting and reply writing.
//!
//! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
//! - The request head is read in a capped loop (16 KiB, [`MAX_HEAD_BYTES`])
//!   under one 10 s total budget; overflow → `431 Request Header Fields
//!   Too Large`, garbage → `400 Bad Request`, both followed by a close.
//! - CONNECT: authority-form target (port required), reply
//!   `200 Connection established` only after the upstream tunnel exists,
//!   `502 Bad Gateway` when it fails, `403 Forbidden` when the router
//!   rejects. Bytes already read past the CONNECT head are forwarded into
//!   the tunnel (early client data must not be dropped).
//! - Plain requests: absolute-form `http://` and origin-form (with a Host
//!   header) are accepted; the head is rewritten to origin-form with
//!   `Proxy-*` headers stripped (they are hop-by-hop and may carry proxy
//!   credentials that must not leak upstream). The connection then becomes
//!   a raw pipe, so pipelined follow-up requests pass through verbatim
//!   (only the FIRST head is rewritten — documented limitation).
//! - Absolute-form `https://` is rejected with 400: a plain HTTP proxy
//!   speaks TLS neither to the client nor upstream; clients use CONNECT.
//! - No DNS resolution: absolute-form targets keep the authority as parsed.

use std::net::{IpAddr, Ipv6Addr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::{Host, NetError, TargetAddr};

/// Total budget for reading one request head (covers byte-dripping clients).
pub(crate) const REQUEST_HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// Hard cap on a request head; beyond this we answer 431 and close.
pub(crate) const MAX_HEAD_BYTES: usize = 16 * 1024;

pub(crate) const RESP_OK_CONNECT: &str = "HTTP/1.1 200 Connection established\r\n\r\n";
pub(crate) const RESP_502: &str =
    "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
pub(crate) const RESP_403: &str =
    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
pub(crate) const RESP_400: &str =
    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
pub(crate) const RESP_431: &str =
    "HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

/// One parsed inbound HTTP proxy request.
#[derive(Debug)]
pub(crate) struct ParsedRequest {
    /// CONNECT (tunnel) vs plain (forward) request.
    pub is_connect: bool,
    /// Where the tunnel/forward goes.
    pub target: TargetAddr,
    /// Bytes to write upstream immediately after connecting: the rewritten
    /// request head plus any early body bytes (`None` for CONNECT).
    pub forward: Option<Vec<u8>>,
    /// Bytes read past the CONNECT head (early tunnel data).
    pub tunnel_prefix: Vec<u8>,
}

/// Read + parse one request. Protocol errors are answered (431/400) before
/// `Err` is returned; the caller just drops the stream.
pub(crate) async fn read_request(stream: &mut TcpStream) -> Result<ParsedRequest, NetError> {
    match timeout(REQUEST_HEAD_TIMEOUT, read_head_inner(stream)).await {
        Ok(r) => r,
        Err(_) => Err(NetError::Http("timed out reading request head".into())),
    }
}

async fn read_head_inner(stream: &mut TcpStream) -> Result<ParsedRequest, NetError> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        if buf.len() > MAX_HEAD_BYTES {
            let _ = write_simple(stream, RESP_431).await;
            return Err(NetError::Http(format!(
                "request head exceeds {MAX_HEAD_BYTES} bytes"
            )));
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(NetError::Http(
                "client closed before the request head was complete".into(),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_head_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..pos]).into_owned();
            let leftover = buf[pos + 4..].to_vec();
            let parsed = match parse_request(&head, leftover) {
                Ok(p) => p,
                Err(e) => {
                    let _ = write_simple(stream, RESP_400).await;
                    return Err(e);
                }
            };
            return Ok(parsed);
        }
    }
}

/// Locate the `\r\n\r\n` head terminator; returns the index of its `\r`.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Parse a request head (pure; `leftover` is what was read past the head).
pub(crate) fn parse_request(head: &str, leftover: Vec<u8>) -> Result<ParsedRequest, NetError> {
    // A head whose request line is the ONLY line (no headers, e.g.
    // "CONNECT host:port HTTP/1.1") arrives here WITHOUT its final
    // "\r\n\r\n" — `read_head_inner` already stripped the terminator — so
    // the whole head is the request line and the header block is empty.
    let (req_line, headers) = match head.split_once("\r\n") {
        Some((l, h)) => (l, h),
        None => (head, ""),
    };
    let mut parts = req_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| NetError::Http("empty request line".into()))?;
    let target_s = parts
        .next()
        .ok_or_else(|| NetError::Http("request line missing target".into()))?;
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1") {
        return Err(NetError::Http(format!("unsupported version '{version}'")));
    }

    if method.eq_ignore_ascii_case("CONNECT") {
        let target = parse_authority(target_s, None)?;
        return Ok(ParsedRequest {
            is_connect: true,
            target,
            forward: None,
            tunnel_prefix: leftover,
        });
    }

    let host_hdr = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("host")
            .then(|| value.trim().to_string())
    });

    let (authority, origin_form) = if let Some(rest) = strip_scheme(target_s, "http://") {
        let (a, path) = match rest.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (rest, None),
        };
        (a.to_string(), format!("/{}", path.unwrap_or("")))
    } else if target_s.starts_with('/') {
        let host = host_hdr
            .ok_or_else(|| NetError::Http("origin-form request without Host header".into()))?;
        (host, target_s.to_string())
    } else if strip_scheme(target_s, "https://").is_some() {
        return Err(NetError::Http(
            "absolute-form https is not supported; use CONNECT".into(),
        ));
    } else {
        return Err(NetError::Http(format!(
            "unsupported request target '{target_s}'"
        )));
    };

    let target = parse_authority(&authority, Some(80))?;
    let forward = rebuild_request_head(method, &origin_form, headers, &leftover);
    Ok(ParsedRequest {
        is_connect: false,
        target,
        forward: Some(forward),
        tunnel_prefix: Vec::new(),
    })
}

/// Case-insensitive scheme strip preserving the original tail.
fn strip_scheme<'a>(target: &'a str, scheme: &str) -> Option<&'a str> {
    if target.len() >= scheme.len() && target[..scheme.len()].eq_ignore_ascii_case(scheme) {
        Some(&target[scheme.len()..])
    } else {
        None
    }
}

/// Rebuild the head for upstream: origin-form request line, original header
/// lines minus `Proxy-*` (hop-by-hop), final blank line, then any bytes
/// already read past the head (early body).
fn rebuild_request_head(
    method: &str,
    origin_form: &str,
    headers: &str,
    leftover: &[u8],
) -> Vec<u8> {
    let mut out = format!("{method} {origin_form} HTTP/1.1\r\n");
    for line in headers.lines() {
        if line.is_empty() {
            continue;
        }
        let name = line.split_once(':').map_or("", |(n, _)| n.trim());
        if name.to_ascii_lowercase().starts_with("proxy-") {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(leftover);
    bytes
}

/// Parse `authority` into a [`TargetAddr`]. Handles `[v6]:port`, bare IP
/// literals, `host:port` and host-with-`default_port`. Bare (unbracketed)
/// IPv6 is rejected.
pub(crate) fn parse_authority(s: &str, default_port: Option<u16>) -> Result<TargetAddr, NetError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(NetError::Target("empty authority".into()));
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (v6, after) = rest
            .split_once(']')
            .ok_or_else(|| NetError::Target(format!("unterminated [ipv6] in '{s}'")))?;
        let ip: Ipv6Addr = v6
            .parse()
            .map_err(|_| NetError::Target(format!("invalid ipv6 '{v6}'")))?;
        let port = port_from(after.strip_prefix(':'), default_port, s)?;
        return Ok(TargetAddr {
            host: Host::Ip(IpAddr::V6(ip)),
            port,
        });
    }
    // Bare IP literal covering the whole authority. IPv4 is legal with or
    // without a port; a bare (unbracketed) IPv6 is NOT a legal authority
    // (RFC 3986: an IP-literal in the authority MUST use the bracketed
    // form) and would be ambiguous with `host:port`, so reject it.
    if let Ok(ip) = s.parse::<IpAddr>() {
        if ip.is_ipv6() {
            return Err(NetError::Target(format!(
                "bare ipv6 '{s}' must be bracketed"
            )));
        }
        let port =
            default_port.ok_or_else(|| NetError::Target(format!("missing port in '{s}'")))?;
        return Ok(TargetAddr {
            host: Host::Ip(ip),
            port,
        });
    }
    let (host, port_s) = match s.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (s, None),
    };
    // `host` is the text before the FIRST ':', so it can never contain a
    // colon itself; leftover bare-ipv6-shaped garbage therefore fails the
    // port parse below. An IPv4 literal WITH an explicit port (e.g. the
    // CONNECT authority form "127.0.0.1:9000") must yield [`Host::Ip`],
    // not a numeric domain string.
    if let Ok(ip) = host.parse::<IpAddr>() {
        let port = port_from(port_s, default_port, s)?;
        return Ok(TargetAddr {
            host: Host::Ip(ip),
            port,
        });
    }
    validate_domain(host)?;
    let port = port_from(port_s, default_port, s)?;
    Ok(TargetAddr {
        host: Host::Domain(host.to_string()),
        port,
    })
}

fn port_from(p: Option<&str>, default: Option<u16>, ctx: &str) -> Result<u16, NetError> {
    match p {
        Some(ps) => ps
            .parse::<u16>()
            .map_err(|_| NetError::Target(format!("invalid port '{ps}' in '{ctx}'"))),
        None => default.ok_or_else(|| NetError::Target(format!("missing port in '{ctx}'"))),
    }
}

fn validate_domain(host: &str) -> Result<(), NetError> {
    if host.is_empty() {
        return Err(NetError::Target("empty host".into()));
    }
    let b = host.as_bytes();
    if b.len() > 255 {
        return Err(NetError::Target(format!(
            "host too long ({} bytes, wire max 255)",
            b.len()
        )));
    }
    if b.iter()
        .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return Err(NetError::Target(
            "host contains whitespace or control characters".into(),
        ));
    }
    Ok(())
}

/// Write a canned single-shot response line + close headers.
pub(crate) async fn write_simple(stream: &mut TcpStream, resp: &str) -> std::io::Result<()> {
    stream.write_all(resp.as_bytes()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_authority_matrix() {
        // host:port (domain)
        let t = parse_authority("example.org:8080", None).unwrap();
        assert_eq!(t.host, Host::Domain("example.org".into()));
        assert_eq!(t.port, 8080);
        // host + default port
        let t = parse_authority("example.org", Some(80)).unwrap();
        assert_eq!(t.port, 80);
        assert!(parse_authority("example.org", None).is_err());
        // bare IPv4 + default
        let t = parse_authority("127.0.0.1", Some(80)).unwrap();
        assert_eq!(t.host, Host::Ip("127.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(parse_authority("127.0.0.1", None).is_err());
        // bracketed IPv6 with and without port
        let t = parse_authority("[2001:db8::1]:443", None).unwrap();
        assert_eq!(t.host, Host::Ip("2001:db8::1".parse::<IpAddr>().unwrap()));
        assert_eq!(t.port, 443);
        let t = parse_authority("[2001:db8::1]", Some(80)).unwrap();
        assert_eq!(t.port, 80);
        // bare IPv6 rejected
        assert!(parse_authority("2001:db8::1", Some(80)).is_err());
        // invalid ports / whitespace hosts rejected
        assert!(parse_authority("example.org:notaport", Some(80)).is_err());
        assert!(parse_authority("bad host.org", Some(80)).is_err());
        assert!(parse_authority("", Some(80)).is_err());
    }

    #[test]
    fn parse_connect_request_authority_form() {
        let p = parse_request(
            "CONNECT 127.0.0.1:9000 HTTP/1.1\r\nHost: 127.0.0.1:9000",
            b"early".to_vec(),
        )
        .unwrap();
        assert!(p.is_connect);
        assert_eq!(p.target.host, Host::Ip("127.0.0.1".parse().unwrap()));
        assert_eq!(p.target.port, 9000);
        assert!(p.forward.is_none());
        assert_eq!(p.tunnel_prefix, b"early");
        // CONNECT without a port is invalid.
        assert!(parse_request("CONNECT example.org HTTP/1.1\r\n", Vec::new()).is_err());
    }

    #[test]
    fn parse_absolute_form_rewrites_and_strips_proxy_headers() {
        let head = "GET http://example.org/p?q=1 HTTP/1.1\r\n\
                    Host: example.org\r\n\
                    Proxy-Authorization: Basic c3ludGhldGlj\r\n\
                    User-Agent: mt-net-test\r\n";
        let p = parse_request(head, b"BODY".to_vec()).unwrap();
        assert!(!p.is_connect);
        assert_eq!(p.target.host, Host::Domain("example.org".into()));
        assert_eq!(p.target.port, 80);
        let fwd = p.forward.unwrap();
        let text = String::from_utf8(fwd).unwrap();
        assert!(text.starts_with("GET /p?q=1 HTTP/1.1\r\n"), "got: {text}");
        assert!(text.contains("Host: example.org"));
        assert!(text.contains("User-Agent: mt-net-test"));
        assert!(!text.contains("Proxy-Authorization"), "hop-by-hop leak");
        assert!(text.ends_with("\r\n\r\nBODY"));
    }

    #[test]
    fn parse_origin_form_uses_host_header() {
        let head = "POST /upload HTTP/1.1\r\nHost: origin.test:8123\r\nContent-Length: 0";
        let p = parse_request(head, Vec::new()).unwrap();
        assert_eq!(p.target.host, Host::Domain("origin.test".into()));
        assert_eq!(p.target.port, 8123);
        // Origin-form without Host → 400 path.
        assert!(parse_request("GET / HTTP/1.1\r\nX: y", Vec::new()).is_err());
    }

    #[test]
    fn parse_rejects_https_absolute_and_garbage() {
        assert!(parse_request("GET https://example.org/ HTTP/1.1\r\nHost: x", Vec::new()).is_err());
        assert!(parse_request("NONSENSE\r\n\r\n", Vec::new()).is_err());
        assert!(parse_request("GET / HTTP/2.0\r\nHost: x", Vec::new()).is_err());
    }
}
