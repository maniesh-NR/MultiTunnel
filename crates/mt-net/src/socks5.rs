//! SOCKS5 client ([`Socks5Outbound`]) and SOCKS5 server handshake helpers.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"):
//! - CLIENT: `Socks5Outbound { addr: SocketAddr }` — no-auth SOCKS5 client
//!   whose wire discipline is ported from the root crate's
//!   `src/core_proc.rs::socks_probe` (greeting `05 01 00` → expect `05 00`;
//!   CONNECT with the ATYP that matches the host literal — `01` + 4 bytes
//!   for IPv4, `04` + 16 bytes for IPv6, `03` + len + domain — then reply
//!   status check and BND.ADDR/BND.PORT drain per ATYP), but async and
//!   returning the established stream instead of dropping it.
//! - SERVER (handshake half here; connect/tunnel flow in `server.rs`):
//!   RFC 1928, no-auth only, CONNECT only (BIND/UDP get proper error
//!   replies), domain + IPv4 + IPv6 ATYP, correct full replies
//!   (VER REP RSV ATYP BND.ADDR BND.PORT), concurrent connections, read
//!   timeouts.
//!
//! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
//! - ONE total handshake budget wraps greeting+request on both sides
//!   (client 10 s, server 10 s) instead of per-read timeouts — same
//!   byte-dripping-peer argument as `socks_probe` (S2-REV-C P1-1).
//! - Server success/error replies use ATYP 01 with zero BND.ADDR/BND.PORT
//!   (RFC 1928 allows any BND value for CONNECT; xray/v2ray do the same);
//!   the client drains per the REPLY's ATYP, so both directions agree.
//! - Server-side failure mapping (best-effort, documented): router Reject →
//!   REP 02 (connection not allowed by ruleset), upstream ConnectionRefused
//!   → REP 05, upstream TimedOut → REP 03, everything else → REP 01.
//! - No-auth not offered → server answers `05 FF` (NO ACCEPTABLE METHODS)
//!   and closes; wrong version → silent close (nothing sane to reply).
//! - Domains: client side validates non-empty and ≤255 wire bytes; server
//!   side the wire length byte already bounds it and non-UTF-8 domains are
//!   rejected (REP 01) rather than lossily re-encoded.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::{Host, NetError, Outbound, ProxyStream, TargetAddr};

/// SOCKS5 protocol version (RFC 1928).
const VER: u8 = 0x05;
/// Auth method: no authentication required.
const NO_AUTH: u8 = 0x00;
/// The only request command we implement: CONNECT.
const CMD_CONNECT: u8 = 0x01;
/// Address types (RFC 1928 §5).
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

/// SOCKS5 REP codes we emit (RFC 1928 §6).
pub(crate) mod rep {
    /// General SOCKS server failure.
    pub const GENERAL_FAILURE: u8 = 0x01;
    /// Connection not allowed by ruleset (router Reject).
    pub const NOT_ALLOWED: u8 = 0x02;
    /// Network unreachable.
    pub const NET_UNREACHABLE: u8 = 0x03;
    /// Connection refused.
    pub const CONNECTION_REFUSED: u8 = 0x05;
    /// Command not supported (BIND / UDP ASSOCIATE).
    pub const CMD_NOT_SUPPORTED: u8 = 0x07;
    /// Address type not supported.
    pub const ATYP_NOT_SUPPORTED: u8 = 0x08;
}

/// Method-selection reply byte: NO ACCEPTABLE METHODS (RFC 1928 §3).
const METHOD_REJECTED: u8 = 0xFF;

/// Total budget for the client-side handshake (greeting + CONNECT).
pub const CLIENT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Total budget for the server-side handshake per inbound connection.
pub(crate) const SERVER_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// SOCKS5 outbound link: tunnels targets through an upstream SOCKS5
/// no-auth proxy at `addr`. Wire discipline ported from the root crate's
/// `socks_probe` (see module docs).
#[derive(Debug, Clone)]
pub struct Socks5Outbound {
    /// Upstream proxy address.
    pub addr: SocketAddr,
}

impl Socks5Outbound {
    /// Create a SOCKS5 outbound pointing at `addr`.
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    /// Greeting + CONNECT + reply drain; returns the raw tunnel stream.
    /// Total-budgeted (see module docs); the returned stream escapes the
    /// timeout, so live tunnels are never killed by the handshake budget.
    async fn establish(&self, target: &TargetAddr) -> Result<TcpStream, NetError> {
        let fut = async {
            let mut s = TcpStream::connect(self.addr)
                .await
                .map_err(|e| NetError::ProxyUnreachable(self.addr, e))?;
            // S21 P1 perf: this loopback leg carries every tunnel byte
            // (per-connection mode: the whole proxied flow; stream mode:
            // every carrier). Nagle + delayed-ACK turn the pump's 16 KiB
            // writes into stop-and-wait micro-bursts; interactive proxy
            // traffic wants TCP_NODELAY, exactly like the inbound listener.
            let _ = s.set_nodelay(true);
            // Greeting: offer no-auth (00) only.
            s.write_all(&[VER, 0x01, NO_AUTH]).await?;
            let mut g = [0u8; 2];
            s.read_exact(&mut g).await?;
            if g[0] != VER || g[1] != NO_AUTH {
                return Err(NetError::Socks(format!(
                    "greeting refused (ver 0x{:02x}, method 0x{:02x})",
                    g[0], g[1]
                )));
            }
            // CONNECT with the ATYP matching the host literal.
            s.write_all(&build_connect_request(&target.host, target.port)?)
                .await?;
            // Reply head: VER, REP, RSV, ATYP.
            let mut head = [0u8; 4];
            s.read_exact(&mut head).await?;
            if head[0] != VER {
                return Err(NetError::Socks(format!(
                    "bad reply version 0x{:02x}",
                    head[0]
                )));
            }
            if head[1] != 0x00 {
                return Err(NetError::SocksStatus(head[1]));
            }
            // Drain BND.ADDR + BND.PORT per the REPLY's ATYP.
            drain_bound_address(&mut s, head[3]).await?;
            Ok(s)
        };
        match timeout(CLIENT_HANDSHAKE_TIMEOUT, fut).await {
            Ok(r) => r,
            Err(_) => Err(NetError::Socks(format!(
                "handshake with proxy {} timed out",
                self.addr
            ))),
        }
    }
}

#[async_trait::async_trait]
impl Outbound for Socks5Outbound {
    fn name(&self) -> &str {
        "socks5"
    }

    async fn connect(&self, target: &TargetAddr) -> anyhow::Result<ProxyStream> {
        Ok(Box::new(self.establish(target).await?))
    }

    async fn probe(&self, target: &TargetAddr) -> anyhow::Result<Duration> {
        let started = Instant::now();
        let s = self.establish(target).await?;
        drop(s);
        Ok(started.elapsed())
    }
}

/// Build the SOCKS5 CONNECT request for `host`:`port` (byte-exact port of
/// the root crate's `build_connect_request`): `05 01 00` + ATYP section +
/// port (big-endian). ATYP 0x01 + 4 address bytes for an IPv4 literal,
/// 0x04 + 16 bytes for an IPv6 literal, 0x03 + one-byte length + domain
/// otherwise.
pub(crate) fn build_connect_request(host: &Host, port: u16) -> Result<Vec<u8>, NetError> {
    let mut req = Vec::with_capacity(262);
    req.extend_from_slice(&[VER, CMD_CONNECT, 0x00]);
    match host {
        Host::Ip(IpAddr::V4(v4)) => {
            req.push(ATYP_IPV4);
            req.extend_from_slice(&v4.octets());
        }
        Host::Ip(IpAddr::V6(v6)) => {
            req.push(ATYP_IPV6);
            req.extend_from_slice(&v6.octets());
        }
        Host::Domain(d) => {
            let b = d.as_bytes();
            if b.is_empty() {
                return Err(NetError::Target("empty domain".into()));
            }
            if b.len() > 255 {
                return Err(NetError::Target(format!(
                    "domain too long ({} bytes, wire max 255)",
                    b.len()
                )));
            }
            req.push(ATYP_DOMAIN);
            req.push(b.len() as u8);
            req.extend_from_slice(b);
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    Ok(req)
}

/// Drain BND.ADDR + BND.PORT following the reply's ATYP (client side).
async fn drain_bound_address(s: &mut TcpStream, atyp: u8) -> std::io::Result<()> {
    match atyp {
        ATYP_IPV4 => {
            let mut b = [0u8; 6];
            s.read_exact(&mut b).await?;
        }
        ATYP_DOMAIN => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            let mut b = vec![0u8; usize::from(l[0]) + 2];
            s.read_exact(&mut b).await?;
        }
        ATYP_IPV6 => {
            let mut b = [0u8; 18];
            s.read_exact(&mut b).await?;
        }
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown reply ATYP 0x{other:02x}"),
            ));
        }
    }
    Ok(())
}

/// Full RFC 1928 server handshake on an accepted inbound stream: greeting
/// (no-auth only) + CONNECT request parse. Protocol violations get the
/// proper error reply written best-effort before `Err` is returned; on
/// `Ok` the success reply is the CALLER's job (it is sent only once an
/// upstream tunnel exists). Total-budgeted (see module docs).
pub(crate) async fn server_handshake(
    stream: &mut TcpStream,
    budget: Duration,
) -> Result<TargetAddr, NetError> {
    match timeout(budget, handshake_inner(stream)).await {
        Ok(r) => r,
        Err(_) => Err(NetError::Socks("inbound handshake timed out".into())),
    }
}

async fn handshake_inner(s: &mut TcpStream) -> Result<TargetAddr, NetError> {
    // --- method selection ---
    let mut ver_n = [0u8; 2];
    s.read_exact(&mut ver_n).await?;
    if ver_n[0] != VER {
        return Err(NetError::Socks(format!(
            "client greeted with version 0x{:02x}",
            ver_n[0]
        )));
    }
    let n = usize::from(ver_n[1]);
    if n == 0 {
        let _ = send_method_rejected(s).await;
        return Err(NetError::Socks("client offered no methods".into()));
    }
    let mut methods = vec![0u8; n];
    s.read_exact(&mut methods).await?;
    if !methods.contains(&NO_AUTH) {
        let _ = send_method_rejected(s).await;
        return Err(NetError::Socks("client does not offer no-auth".into()));
    }
    s.write_all(&[VER, NO_AUTH]).await?;

    // --- request ---
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[0] != VER {
        return Err(NetError::Socks(format!(
            "request version 0x{:02x}",
            head[0]
        )));
    }
    if head[1] != CMD_CONNECT {
        let _ = send_error_reply(s, rep::CMD_NOT_SUPPORTED).await;
        return Err(NetError::Socks(format!(
            "command 0x{:02x} not supported (CONNECT only)",
            head[1]
        )));
    }
    let host = match head[3] {
        ATYP_IPV4 => {
            let mut b = [0u8; 4];
            s.read_exact(&mut b).await?;
            Host::Ip(IpAddr::V4(Ipv4Addr::from(b)))
        }
        ATYP_DOMAIN => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            let mut d = vec![0u8; usize::from(l[0])];
            if d.is_empty() {
                let _ = send_error_reply(s, rep::GENERAL_FAILURE).await;
                return Err(NetError::Socks("zero-length domain".into()));
            }
            s.read_exact(&mut d).await?;
            match String::from_utf8(d) {
                Ok(dom) => Host::Domain(dom),
                Err(_) => {
                    let _ = send_error_reply(s, rep::GENERAL_FAILURE).await;
                    return Err(NetError::Socks("non-utf8 domain".into()));
                }
            }
        }
        ATYP_IPV6 => {
            let mut b = [0u8; 16];
            s.read_exact(&mut b).await?;
            Host::Ip(IpAddr::V6(Ipv6Addr::from(b)))
        }
        other => {
            let _ = send_error_reply(s, rep::ATYP_NOT_SUPPORTED).await;
            return Err(NetError::Socks(format!(
                "address type 0x{other:02x} not supported"
            )));
        }
    };
    let mut p = [0u8; 2];
    s.read_exact(&mut p).await?;
    Ok(TargetAddr {
        host,
        port: u16::from_be_bytes(p),
    })
}

/// Method-selection reply: NO ACCEPTABLE METHODS (`05 FF`, 2 bytes).
async fn send_method_rejected(s: &mut TcpStream) -> std::io::Result<()> {
    s.write_all(&[VER, METHOD_REJECTED]).await
}

/// Success reply: `05 00 00 01` + zero BND.ADDR(v4) + zero BND.PORT.
pub(crate) async fn send_success_reply(s: &mut TcpStream) -> std::io::Result<()> {
    s.write_all(&[VER, 0x00, 0x00, ATYP_IPV4, 0, 0, 0, 0, 0, 0])
        .await
}

/// Error reply: `05 <rep> 00 01` + zero BND.ADDR(v4) + zero BND.PORT.
pub(crate) async fn send_error_reply(s: &mut TcpStream, rep: u8) -> std::io::Result<()> {
    s.write_all(&[VER, rep, 0x00, ATYP_IPV4, 0, 0, 0, 0, 0, 0])
        .await
}

/// Best-effort REP mapping for an upstream connect error (see module docs).
pub(crate) fn rep_for_error(err: &anyhow::Error) -> u8 {
    if let Some(ne) = err.downcast_ref::<NetError>() {
        return match ne {
            NetError::Io(e) => rep_for_io(e),
            _ => rep::GENERAL_FAILURE,
        };
    }
    if let Some(e) = err.downcast_ref::<std::io::Error>() {
        return rep_for_io(e);
    }
    rep::GENERAL_FAILURE
}

fn rep_for_io(err: &std::io::Error) -> u8 {
    match err.kind() {
        std::io::ErrorKind::ConnectionRefused => rep::CONNECTION_REFUSED,
        std::io::ErrorKind::TimedOut => rep::NET_UNREACHABLE,
        _ => rep::GENERAL_FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ProxyServer;
    use crate::test_support::{wait_for_addr, FakeOutbound};
    use crate::MergePolicy;
    use std::sync::Arc;
    use std::time::Duration;

    // --- wire format (ported discipline from core_proc::socks_probe) ---

    #[test]
    fn connect_request_uses_atyp_matching_host_kind() {
        let port = 0x0102;
        // IPv4 literal → ATYP 01 + 4 bytes.
        let v4 = build_connect_request(&Host::Ip("127.0.0.1".parse().unwrap()), port).unwrap();
        assert_eq!(v4, vec![0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x01, 0x02]);
        // IPv6 literal → ATYP 04 + 16 bytes.
        let v6 = build_connect_request(&Host::Ip("2001:db8::1".parse().unwrap()), port).unwrap();
        assert_eq!(v6[3], ATYP_IPV6);
        assert_eq!(v6.len(), 4 + 16 + 2);
        assert_eq!(
            &v6[4..20],
            &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
        );
        // Domain → ATYP 03 + len + bytes.
        let dom = build_connect_request(&Host::Domain("tunnel.example.org".into()), port).unwrap();
        assert_eq!(dom[3], ATYP_DOMAIN);
        assert_eq!(dom[4], 18);
        assert_eq!(&dom[5..23], b"tunnel.example.org");
        assert_eq!(&dom[23..], &[0x01, 0x02]);
    }

    #[test]
    fn connect_request_rejects_out_of_wire_domains() {
        let long = "a".repeat(256);
        assert!(build_connect_request(&Host::Domain(long), 80).is_err());
        assert!(build_connect_request(&Host::Domain(String::new()), 80).is_err());
    }

    // --- client↔server loopback E2E against an echo server (contract) ---

    /// Spawns the E2E server. Returns the shutdown sender LAST so callers
    /// can hold it (bound to a named variable) for the whole test: `run()`
    /// treats a DROPPED sender as shutdown, which would close the listeners
    /// while the test is still connecting.
    async fn e2e_server() -> (
        Arc<ProxyServer>,
        SocketAddr,
        SocketAddr,
        tokio::sync::watch::Sender<bool>,
    ) {
        let echo = crate::test_support::spawn_echo().await;
        let links: Vec<Arc<dyn Outbound>> = vec![Arc::new(FakeOutbound::echo("link-a", echo))];
        let server = Arc::new(ProxyServer::new(
            "127.0.0.1:0".parse().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
            links,
            crate::router::Router::new(Vec::new()),
            MergePolicy::Failover,
        ));
        let (tx, rx) = tokio::sync::watch::channel(false);
        tokio::spawn(server.clone().run(rx));
        let socks_addr = wait_for_addr(|| server.socks_local_addr()).await;
        (server, socks_addr, echo, tx)
    }

    #[tokio::test]
    async fn client_server_loopback_e2e_ip_target() {
        let (server, socks_addr, echo, _shutdown) = e2e_server().await;
        let cli = Socks5Outbound::new(socks_addr);
        let target = TargetAddr {
            host: Host::Ip(echo.ip()),
            port: echo.port(),
        };
        let mut s = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
            .await
            .expect("connect in time")
            .expect("socks e2e connect ok");
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        s.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
        drop(server); // keep alive until here
    }

    #[tokio::test]
    async fn client_server_loopback_e2e_domain_target() {
        let (server, socks_addr, echo, _shutdown) = e2e_server().await;
        let cli = Socks5Outbound::new(socks_addr);
        // Domain ATYP both directions; the fake link ignores the target, so
        // "synthetic.test" exercises encode(server)+decode(client) only.
        let target = TargetAddr {
            host: Host::Domain("synthetic.test".into()),
            port: echo.port(),
        };
        let mut s = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
            .await
            .expect("connect in time")
            .expect("socks e2e connect ok");
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        s.write_all(b"pong").await.unwrap();
        let mut buf = [0u8; 4];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"pong");
        drop(server);
    }

    // --- server-side protocol replies ---

    #[tokio::test]
    async fn server_replies_07_to_bind_command() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let (server, socks_addr, _echo, _shutdown) = e2e_server().await;
        let mut s = TcpStream::connect(socks_addr).await.unwrap();
        s.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut m = [0u8; 2];
        s.read_exact(&mut m).await.unwrap();
        assert_eq!(m, [0x05, 0x00]);
        // BIND (CMD 02) → REP 07 command not supported.
        s.write_all(&[0x05, 0x02, 0x00, 0x01, 127, 0, 0, 1, 0x00, 0x50])
            .await
            .unwrap();
        let mut r = [0u8; 10];
        s.read_exact(&mut r).await.unwrap();
        assert_eq!(r[0], 0x05);
        assert_eq!(r[1], rep::CMD_NOT_SUPPORTED);
        drop(server);
    }

    #[tokio::test]
    async fn server_replies_08_to_unknown_atyp() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let (server, socks_addr, _echo, _shutdown) = e2e_server().await;
        let mut s = TcpStream::connect(socks_addr).await.unwrap();
        s.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut m = [0u8; 2];
        s.read_exact(&mut m).await.unwrap();
        assert_eq!(m, [0x05, 0x00]);
        // ATYP 0x09 → REP 08 address type not supported.
        s.write_all(&[0x05, 0x01, 0x00, 0x09]).await.unwrap();
        let mut r = [0u8; 10];
        s.read_exact(&mut r).await.unwrap();
        assert_eq!(r[1], rep::ATYP_NOT_SUPPORTED);
        drop(server);
    }

    #[tokio::test]
    async fn server_replies_ff_when_noauth_not_offered() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let (server, socks_addr, _echo, _shutdown) = e2e_server().await;
        let mut s = TcpStream::connect(socks_addr).await.unwrap();
        // Offer only username/password (02) → NO ACCEPTABLE METHODS.
        s.write_all(&[0x05, 0x01, 0x02]).await.unwrap();
        let mut m = [0u8; 2];
        s.read_exact(&mut m).await.unwrap();
        assert_eq!(m, [0x05, METHOD_REJECTED]);
        drop(server);
    }

    // --- client-side error surfacing ---

    #[tokio::test]
    async fn client_surfaces_greeting_refusal() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        // Stub proxy that immediately replies 05 FF and closes.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                // Windows RST rule (CI 2026-11-17): close ONLY after the
                // client's greeting was consumed — an unread receive
                // buffer turns the close into an RST that races the reply.
                let mut g = [0u8; 3];
                let _ = s.read_exact(&mut g).await;
                let _ = s.write_all(&[0x05, 0xFF]).await;
                let _ = s.shutdown().await;
                let mut sink = [0u8; 64];
                let _ = s.read(&mut sink).await;
            }
        });
        let cli = Socks5Outbound::new(addr);
        let err = cli
            .connect(&TargetAddr {
                host: Host::Domain("synthetic.test".into()),
                port: 80,
            })
            .await
            .err()
            .expect("greeting refusal must surface");
        let ne = err.downcast_ref::<NetError>().expect("NetError");
        assert!(matches!(ne, NetError::Socks(_)), "got: {ne}");
    }

    #[tokio::test]
    async fn client_surfaces_proxy_status_code() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        // Stub proxy that completes the greeting then refuses CONNECT.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut s, _)) = listener.accept().await {
                let mut g = [0u8; 3];
                if s.read_exact(&mut g).await.is_err() {
                    return;
                }
                if s.write_all(&[0x05, 0x00]).await.is_err() {
                    return;
                }
                // Full CONNECT request for "dead.beef" (9 bytes): head 4 +
                // len 1 + 9 + port 2 = 16 bytes. The old 14-byte read left
                // 2 bytes unread at close → Windows RST raced the reply
                // (CI 2026-11-17).
                let mut req = [0u8; 16];
                let _ = s.read_exact(&mut req).await;
                let _ = send_error_reply(&mut s, rep::CONNECTION_REFUSED).await;
                // Graceful close: shutdown the write side, then drain.
                let _ = s.shutdown().await;
                let mut sink = [0u8; 64];
                let _ = s.read(&mut sink).await;
            }
        });
        let cli = Socks5Outbound::new(addr);
        let err = cli
            .connect(&TargetAddr {
                host: Host::Domain("dead.beef".into()),
                port: 80,
            })
            .await
            .err()
            .expect("refusal must surface");
        let ne = err.downcast_ref::<NetError>().expect("NetError");
        assert!(
            matches!(ne, NetError::SocksStatus(0x05)),
            "expected status 0x05, got: {ne}"
        );
    }
}
