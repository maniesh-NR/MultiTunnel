//! Wire framing for stream bonding (D17).
//!
//! One carrier (a plain TCP stream riding a VLESS+REALITY tunnel through
//! one adapter) carries an arbitrary interleaving of frames for an
//! arbitrary number of sessions. Every frame shares ONE fixed header so
//! decoding stays stateless per carrier:
//!
//! ```text
//! offset  size  field
//! 0       2     magic    u16 LE = 0x4D54 ("MT")
//! 2       1     version  u8  = 1
//! 3       1     kind     u8  (see [`Kind`])
//! 4       4     bond_id  u32 LE (groups carriers of one bonded tunnel)
//! 8       4     session  u32 LE (0 = bond-level control: PING/PONG)
//! 12      8     seq      u64 LE (per session, per direction)
//! 20      4     length   u32 LE (payload bytes; DATA ≤ MAX_PAYLOAD)
//! ```
//!
//! DATA sequence numbers start at 0 for every session and increment by
//! exactly 1 per frame, per direction. OPEN carries the target address in
//! its payload and consumes seq 0 of the client→server direction (the
//! server never sends OPEN). CLOSE payloads are exactly one [`CloseReason`]
//! byte. PING/PONG use session 0 and a bond-scoped seq; the PING payload
//! is an 8-byte LE send timestamp echoed verbatim by PONG.

use std::io;

use mt_net::{Host, TargetAddr};

/// Frame magic ("MT", little-endian on the wire).
pub const MAGIC: u16 = 0x4D54;
/// Wire protocol version.
pub const VERSION: u8 = 1;
/// Fixed header size in bytes.
pub const HEADER_LEN: usize = 24;
/// Maximum payload a single frame may carry (16 KiB — small enough to
/// spread smoothly across carriers, large enough to amortize the header).
pub const MAX_PAYLOAD: usize = 16 * 1024;
/// Session id reserved for bond-level control frames (PING/PONG).
pub const CONTROL_SESSION: u32 = 0;

/// Frame kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// First frame on a carrier; payload = carrier display name.
    Hello = 1,
    /// Open a session; payload = target address; client→server only.
    Open = 2,
    /// Session payload bytes.
    Data = 3,
    /// Session teardown; payload = one [`CloseReason`] byte.
    Close = 4,
    /// Cumulative ack: "all my DATA frames for `session` with seq ≤ `seq`
    /// were received in order".
    Ack = 5,
    /// Carrier liveness probe; payload = 8-byte LE send timestamp.
    Ping = 6,
    /// Ping echo; payload echoed verbatim.
    Pong = 7,
}

impl Kind {
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Hello,
            2 => Self::Open,
            3 => Self::Data,
            4 => Self::Close,
            5 => Self::Ack,
            6 => Self::Ping,
            7 => Self::Pong,
            _ => return None,
        })
    }
}

/// Why a session ended (CLOSE payload byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CloseReason {
    /// Clean end of stream from the peer.
    Eof = 0,
    /// The receiving side does not know this session.
    UnknownSession = 1,
    /// Reorder gap made no progress within the stall window.
    Stall = 2,
    /// The daemon could not dial the requested target.
    Refused = 3,
    /// Reorder buffer overflow or internal error.
    Error = 4,
}

impl CloseReason {
    pub(crate) fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Eof,
            1 => Self::UnknownSession,
            2 => Self::Stall,
            3 => Self::Refused,
            _ => Self::Error,
        }
    }

    /// Human text for logs and UI error strings.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eof => "closed by peer",
            Self::UnknownSession => "unknown session",
            Self::Stall => "stream stalled (link gap)",
            Self::Refused => "target refused",
            Self::Error => "bond error",
        }
    }
}

/// One decoded (or encodable) frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind,
    pub bond_id: u32,
    pub session: u32,
    pub seq: u64,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Build a control (session 0) frame.
    pub fn control(kind: Kind, bond_id: u32, seq: u64, payload: Vec<u8>) -> Self {
        Self {
            kind,
            bond_id,
            session: CONTROL_SESSION,
            seq,
            payload,
        }
    }

    /// Build a session frame.
    pub fn session(kind: Kind, bond_id: u32, session: u32, seq: u64, payload: Vec<u8>) -> Self {
        Self {
            kind,
            bond_id,
            session,
            seq,
            payload,
        }
    }

    /// Encoded size (header + payload).
    pub fn wire_len(&self) -> usize {
        HEADER_LEN + self.payload.len()
    }

    /// Serialize into `out` (header then payload).
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.reserve(self.wire_len());
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.push(VERSION);
        out.push(self.kind as u8);
        out.extend_from_slice(&self.bond_id.to_le_bytes());
        out.extend_from_slice(&self.session.to_le_bytes());
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
    }

    /// Serialize into a fresh buffer.
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.wire_len());
        self.encode(&mut v);
        v
    }
}

/// Decode a frame header from exactly [`HEADER_LEN`] bytes.
pub fn decode_header(buf: &[u8; HEADER_LEN]) -> io::Result<(Kind, u32, u32, u64, usize)> {
    let magic = u16::from_le_bytes([buf[0], buf[1]]);
    if magic != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bond frame magic mismatch: 0x{magic:04X}"),
        ));
    }
    if buf[2] != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bond frame version mismatch: {}", buf[2]),
        ));
    }
    let Some(kind) = Kind::from_u8(buf[3]) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bond frame kind unknown: {}", buf[3]),
        ));
    };
    let bond_id = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let session = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let seq = u64::from_le_bytes([
        buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
    ]);
    let len = u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bond frame payload too large: {len} > {MAX_PAYLOAD}"),
        ));
    }
    Ok((kind, bond_id, session, seq, len))
}

/// Read exactly one frame from an async stream (header, then payload).
pub async fn read_frame<R>(r: &mut R) -> io::Result<Frame>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt as _;
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header).await?;
    let (kind, bond_id, session, seq, len) = decode_header(&header)?;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).await?;
    Ok(Frame {
        kind,
        bond_id,
        session,
        seq,
        payload,
    })
}

/// Write one frame to an async stream and flush.
pub async fn write_frame<W>(w: &mut W, frame: &Frame) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt as _;
    w.write_all(&frame.encode_to_vec()).await?;
    w.flush().await
}

// ---------------------------------------------------------------------------
// OPEN payload: target address encoding (ATYP-style, self-describing)
// ---------------------------------------------------------------------------

const ATYP_DOMAIN: u8 = 0x01;
const ATYP_IPV4: u8 = 0x04;
const ATYP_IPV6: u8 = 0x06;

/// Encode a target address for an OPEN payload.
pub fn encode_target(t: &TargetAddr) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 4 + 262);
    match &t.host {
        Host::Domain(d) => {
            out.push(ATYP_DOMAIN);
            out.extend_from_slice(&(d.len() as u16).to_be_bytes());
            out.extend_from_slice(d.as_bytes());
        }
        Host::Ip(std::net::IpAddr::V4(v4)) => {
            out.push(ATYP_IPV4);
            out.extend_from_slice(&v4.octets());
        }
        Host::Ip(std::net::IpAddr::V6(v6)) => {
            out.push(ATYP_IPV6);
            out.extend_from_slice(&v6.octets());
        }
    }
    out.extend_from_slice(&t.port.to_be_bytes());
    out
}

/// Decode a target address from an OPEN payload.
pub fn decode_target(payload: &[u8]) -> io::Result<TargetAddr> {
    fn need(payload: &[u8], n: usize) -> io::Result<&[u8]> {
        if payload.len() < n {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bond OPEN payload truncated",
            ));
        }
        Ok(&payload[..n])
    }
    let atyp = *need(payload, 1)?.first().expect("1 byte");
    let rest = &payload[1..];
    let (host, rest) = match atyp {
        ATYP_DOMAIN => {
            let len = u16::from_be_bytes([rest[0], rest[1]]) as usize;
            if len == 0 || len > 253 || rest.len() < 2 + len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "bond OPEN domain length out of range",
                ));
            }
            let name = std::str::from_utf8(&rest[2..2 + len])
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bond OPEN domain utf8"))?
                .to_string();
            (Host::Domain(name), &rest[2 + len..])
        }
        ATYP_IPV4 => {
            let b = need(rest, 4)?;
            (
                Host::Ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(
                    b[0], b[1], b[2], b[3],
                ))),
                &rest[4..],
            )
        }
        ATYP_IPV6 => {
            let b: [u8; 16] = need(rest, 16)?.try_into().expect("16 bytes");
            (Host::Ip(std::net::IpAddr::V6(b.into())), &rest[16..])
        }
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("bond OPEN atyp unknown: {other}"),
            ))
        }
    };
    let port = u16::from_be_bytes([rest[0], rest[1]]);
    Ok(TargetAddr { host, port })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: Kind) -> Frame {
        Frame {
            kind,
            bond_id: 0xDEAD_BEEF,
            session: 42,
            seq: 7,
            payload: b"payload bytes".to_vec(),
        }
    }

    #[test]
    fn roundtrip_all_kinds() {
        for kind in [
            Kind::Hello,
            Kind::Open,
            Kind::Data,
            Kind::Close,
            Kind::Ack,
            Kind::Ping,
            Kind::Pong,
        ] {
            let f = sample(kind);
            let bytes = f.encode_to_vec();
            assert_eq!(bytes.len(), f.wire_len());
            let mut hdr = [0u8; HEADER_LEN];
            hdr.copy_from_slice(&bytes[..HEADER_LEN]);
            let (k, bond, sess, seq, len) = decode_header(&hdr).expect("header decodes");
            assert_eq!(k, kind);
            assert_eq!(bond, f.bond_id);
            assert_eq!(sess, f.session);
            assert_eq!(seq, f.seq);
            assert_eq!(len, f.payload.len());
            assert_eq!(&bytes[HEADER_LEN..], f.payload.as_slice());
        }
    }

    #[test]
    fn empty_and_max_payloads_roundtrip() {
        let empty = Frame::control(Kind::Ping, 1, 0, Vec::new());
        let v = empty.encode_to_vec();
        assert_eq!(v.len(), HEADER_LEN);
        let (_, _, _, _, len) = decode_header(&v[..HEADER_LEN].try_into().unwrap()).unwrap();
        assert_eq!(len, 0);

        let big = Frame {
            kind: Kind::Data,
            bond_id: 5,
            session: 9,
            seq: 3,
            payload: vec![0xAB; MAX_PAYLOAD],
        };
        let v = big.encode_to_vec();
        assert_eq!(v.len(), HEADER_LEN + MAX_PAYLOAD);
        let (_, _, _, _, len) = decode_header(&v[..HEADER_LEN].try_into().unwrap()).unwrap();
        assert_eq!(len, MAX_PAYLOAD);
    }

    #[test]
    fn header_rejects_bad_magic_version_kind_and_size() {
        let f = sample(Kind::Data);
        let mut v = f.encode_to_vec();
        v[0] = 0xFF; // magic
        assert!(decode_header(&v[..HEADER_LEN].try_into().unwrap()).is_err());
        let mut v = f.encode_to_vec();
        v[2] = 99; // version
        assert!(decode_header(&v[..HEADER_LEN].try_into().unwrap()).is_err());
        let mut v = f.encode_to_vec();
        v[3] = 99; // kind
        assert!(decode_header(&v[..HEADER_LEN].try_into().unwrap()).is_err());
        let mut v = f.encode_to_vec();
        v[20..24].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
        assert!(decode_header(&v[..HEADER_LEN].try_into().unwrap()).is_err());
    }

    #[test]
    fn target_roundtrip_domain_ipv4_ipv6() {
        let cases = [
            TargetAddr {
                host: Host::Domain("example.com".into()),
                port: 443,
            },
            TargetAddr {
                host: Host::Ip("192.0.2.7".parse().unwrap()),
                port: 8080,
            },
            TargetAddr {
                host: Host::Ip("2001:db8::1".parse().unwrap()),
                port: 65535,
            },
        ];
        for t in cases {
            let enc = encode_target(&t);
            let dec = decode_target(&enc).expect("decodes");
            assert_eq!(dec, t);
        }
    }

    #[test]
    fn target_rejects_truncated_and_garbage() {
        assert!(decode_target(&[]).is_err());
        assert!(decode_target(&[0x04, 1, 2]).is_err()); // truncated v4
        assert!(decode_target(&[0x99, 0, 0]).is_err()); // bad atyp
        assert!(decode_target(&[0x01, 0x00, 0x20, b'a']).is_err()); // domain len lies
    }

    #[tokio::test]
    async fn read_write_frame_over_duplex() {
        let f = sample(Kind::Data);
        let mut pair = tokio::io::duplex(64);
        write_frame(&mut pair.0, &f).await.expect("write");
        let back = read_frame(&mut pair.1).await.expect("read");
        assert_eq!(back, f);
    }
}
