//! Network-adapter enumeration for the GUI's "which links do we bond" menu.
//!
//! CONTRACT (frozen by the orchestrator, S7 stage): [`NetAdapter`] describes
//! one machine adapter and [`enumerate_adapters`] lists every non-loopback
//! adapter with its unicast IPv4 addresses. The engine later binds a chosen
//! adapter's IPv4 via `sendThrough`, so the per-adapter IPv4 list is the
//! load-bearing part of the API. Loopback is always excluded; IPv6 is
//! intentionally omitted (the engine binds IPv4 only). No serde here — the
//! service layer maps this type.
//!
//! IMPLEMENTATION NOTES:
//! * Fail-open everywhere: enumeration NEVER panics and has no error path —
//!   any failure (FFI error, retry exhaustion, unreadable `/proc`) yields an
//!   empty vec, because an enumeration hiccup must not block the app. A
//!   single malformed adapter/address is skipped without aborting the walk.
//! * Windows (production target): `GetAdaptersAddresses` via `windows-sys`,
//!   family AF_INET, skipping anycast/multicast/DNS entries but keeping
//!   FriendlyName. Buffer-retry loop: start at 16 KiB, on
//!   ERROR_BUFFER_OVERFLOW grow to the returned size hint (at least doubled,
//!   so adapter churn cannot force quadratic retries), at most 6 attempts,
//!   hard 16 MiB cap. The buffer is a `Vec<u64>` so the 8-byte alignment of
//!   `IP_ADAPTER_ADDRESSES_LH` is guaranteed. All unsafe FFI lives in small
//!   private helpers with null-pointer fallbacks.
//! * IPv4 ordering: within one adapter, global/normal addresses come first
//!   and 169.254.x.x (APIPA/link-local) last, encounter order preserved.
//! * Non-Windows (dev/test convenience only — the shipped binary is
//!   Windows): pure std, no new dependencies. Interface names from
//!   `/proc/net/dev` ("lo" skipped), up-state from sysfs `operstate`,
//!   description from the sysfs `device/driver` symlink basename
//!   (best-effort). IPv4s come from a tolerant `/proc/net/fib_trie` parser
//!   that needs per-interface header lines; real kernels (≥4.x) emit none,
//!   so on actual Linux hosts `ipv4` stays empty — documented limitation,
//!   Windows is the production path.

use std::net::IpAddr;

/// One physical/virtual network adapter visible to the OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetAdapter {
    /// Stable machine-local id: the Windows ANSI AdapterName (a GUID string
    /// like "{4A67B0F8-...}") or the Linux interface name ("wlan0").
    pub id: String,
    /// Human-friendly display name: Windows FriendlyName ("Wi-Fi"), or the
    /// Linux interface name.
    pub name: String,
    /// Adapter description (Windows Description field; on Linux the sysfs
    /// `device/driver` basename or empty).
    pub description: String,
    /// Unicast IPv4 addresses; global/normal addresses first, link-local
    /// (169.254.x.x / APIPA) last. IPv6 is intentionally omitted (the
    /// engine binds IPv4 only).
    pub ipv4: Vec<std::net::IpAddr>,
    /// True when the OS reports the interface operationally up.
    pub up: bool,
}

/// Enumerate network adapters. NEVER panics; on failure returns an empty
/// vec (fail-open — an enumeration failure must not block the app).
/// Loopback is always excluded.
#[cfg(windows)]
pub fn enumerate_adapters() -> Vec<NetAdapter> {
    ffi::enumerate()
}

/// Enumerate network adapters. NEVER panics; on failure returns an empty
/// vec (fail-open — an enumeration failure must not block the app).
/// Loopback is always excluded.
#[cfg(not(windows))]
pub fn enumerate_adapters() -> Vec<NetAdapter> {
    procfs::enumerate()
}

/// Order unicast IPv4s: global/normal addresses first (encounter order
/// preserved), APIPA/link-local (169.254.x.x) last.
fn order_ipv4(addrs: Vec<IpAddr>) -> Vec<IpAddr> {
    let (mut global, link_local): (Vec<_>, Vec<_>) =
        addrs.into_iter().partition(|ip| !is_apipa(ip));
    global.extend(link_local);
    global
}

/// Is `ip` an IPv4 APIPA/link-local address (169.254.0.0/16)?
fn is_apipa(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if v4.is_link_local())
}

// ---------------------------------------------------------------------------
// Windows: GetAdaptersAddresses FFI (compiled but not exercised in this
// sandbox; cross-compile-verified via cargo zigbuild)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod ffi {
    use super::{order_ipv4, NetAdapter};
    use std::net::{IpAddr, Ipv4Addr};
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN, SOCKET_ADDRESS};

    /// Skip anycast/multicast/DNS entries; FriendlyName stays on (we need it).
    const FLAGS: u32 = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    const INITIAL_BUFFER_BYTES: u32 = 16 * 1024;
    const MAX_ATTEMPTS: usize = 6;
    const MAX_BUFFER_BYTES: u32 = 16 * 1024 * 1024;
    /// Guard for OS-returned NUL-terminated strings (PSTR/PWSTR walks); real
    /// names are far shorter — the cap only protects against a missing NUL.
    const MAX_STR_UNITS: usize = 8192;

    pub(super) fn enumerate() -> Vec<NetAdapter> {
        let mut size = INITIAL_BUFFER_BYTES;
        for _ in 0..MAX_ATTEMPTS {
            // Vec<u64> keeps the 8-byte alignment IP_ADAPTER_ADDRESSES_LH
            // needs; the OS writes its linked list into these bytes.
            let words = size.div_ceil(8) as usize;
            let mut buf: Vec<u64> = vec![0; words];
            let mut size_in_out = (words * 8) as u32;
            let head = buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            let rc = unsafe {
                GetAdaptersAddresses(
                    u32::from(AF_INET),
                    FLAGS,
                    std::ptr::null(),
                    head,
                    &mut size_in_out,
                )
            };
            match rc {
                NO_ERROR => return walk(head),
                ERROR_BUFFER_OVERFLOW => {
                    // Grow to the OS's size hint, at least doubled.
                    let next = size_in_out.max(size.saturating_mul(2));
                    if next > MAX_BUFFER_BYTES {
                        tracing::debug!(required = next, "adapter buffer cap exceeded");
                        return Vec::new();
                    }
                    size = next;
                }
                _ => {
                    tracing::debug!(error = rc, "GetAdaptersAddresses failed");
                    return Vec::new(); // any other error: fail open
                }
            }
        }
        tracing::debug!("GetAdaptersAddresses retries exhausted");
        Vec::new()
    }

    /// Walk the adapter linked list; a malformed entry is skipped, not fatal.
    fn walk(head: *mut IP_ADAPTER_ADDRESSES_LH) -> Vec<NetAdapter> {
        let mut out = Vec::new();
        let mut cur = head;
        while !cur.is_null() {
            let adapter = unsafe { &*cur };
            let next = adapter.Next;
            if let Some(adapter) = convert(adapter) {
                out.push(adapter);
            }
            cur = next;
        }
        out
    }

    /// Map one OS adapter record; `None` = skip (loopback, unusable id).
    fn convert(a: &IP_ADAPTER_ADDRESSES_LH) -> Option<NetAdapter> {
        if a.IfType == IF_TYPE_SOFTWARE_LOOPBACK {
            return None; // loopback is always excluded
        }
        let id = ansi_to_string(a.AdapterName);
        if id.is_empty() {
            return None; // nothing stable to bind — skip but keep walking
        }
        let name = wide_to_string(a.FriendlyName)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| id.clone());
        let description = wide_to_string(a.Description).unwrap_or_default();
        let mut ipv4 = Vec::new();
        let mut ua = a.FirstUnicastAddress;
        while !ua.is_null() {
            let u = unsafe { &*ua };
            let next = u.Next;
            if let Some(ip) = unicast_ipv4(u.Address) {
                ipv4.push(IpAddr::V4(ip));
            }
            ua = next;
        }
        Some(NetAdapter {
            id,
            name,
            description,
            ipv4: order_ipv4(ipv4),
            up: a.OperStatus == IfOperStatusUp,
        })
    }

    /// Extract the IPv4 of one unicast `SOCKET_ADDRESS` (`None` = not v4).
    pub(super) fn unicast_ipv4(sa: SOCKET_ADDRESS) -> Option<Ipv4Addr> {
        let p = sa.lpSockaddr;
        if p.is_null() {
            return None;
        }
        if (sa.iSockaddrLength as usize) < std::mem::size_of::<SOCKADDR_IN>() {
            return None; // truncated/malformed entry — skip
        }
        let sockaddr = unsafe { &*p };
        if sockaddr.sa_family != AF_INET {
            return None; // engine binds IPv4 only
        }
        // Same leading bytes as SOCKADDR; read_unaligned avoids assuming
        // anything about the OS-provided buffer's alignment.
        let v4 = unsafe { p.cast::<SOCKADDR_IN>().read_unaligned() };
        // sin_addr is stored in network byte order (big-endian).
        let s_addr = unsafe { v4.sin_addr.S_un.S_addr };
        Some(Ipv4Addr::from(u32::from_be(s_addr)))
    }

    /// NUL-terminated UTF-16 (PWSTR) → String; `None` on a null pointer.
    pub(super) fn wide_to_string(p: windows_sys::core::PWSTR) -> Option<String> {
        if p.is_null() {
            return None;
        }
        let mut units = Vec::new();
        for i in 0..MAX_STR_UNITS {
            let unit = unsafe { *p.add(i) };
            if unit == 0 {
                return Some(String::from_utf16_lossy(&units));
            }
            units.push(unit);
        }
        None // no NUL within the cap: treat as malformed, do not walk off
    }

    /// NUL-terminated ANSI (PSTR) → String (lossy); "" on a null pointer.
    pub(super) fn ansi_to_string(p: windows_sys::core::PSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut bytes = Vec::new();
        for i in 0..MAX_STR_UNITS {
            let byte = unsafe { *p.add(i) };
            if byte == 0 {
                break;
            }
            bytes.push(byte);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

// ---------------------------------------------------------------------------
// Non-Windows: /proc + /sys (pure std; dev/test convenience only)
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
mod procfs {
    use super::{order_ipv4, NetAdapter};
    use std::collections::BTreeMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::Path;

    pub(super) fn enumerate() -> Vec<NetAdapter> {
        // Interface names are the reliable part of /proc/net/dev.
        let dev = match std::fs::read_to_string("/proc/net/dev") {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "cannot read /proc/net/dev");
                return Vec::new();
            }
        };
        let names = parse_proc_net_dev(&dev);
        if names.is_empty() {
            return Vec::new();
        }
        // Best-effort IPv4s: on kernels without per-interface lines in
        // fib_trie this map is simply empty (see parse_fib_trie).
        let local_ips = std::fs::read_to_string("/proc/net/fib_trie")
            .map(|s| parse_fib_trie(&s))
            .unwrap_or_default();

        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let sys = Path::new("/sys/class/net").join(&name);
            let up = std::fs::read_to_string(sys.join("operstate"))
                .map(|s| s.trim() == "up")
                .unwrap_or(false);
            let description = std::fs::read_link(sys.join("device/driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default();
            let ipv4 = local_ips
                .get(&name)
                .map(|v| v.iter().map(|ip| IpAddr::V4(*ip)).collect())
                .unwrap_or_default();
            out.push(NetAdapter {
                id: name.clone(),
                name,
                description,
                ipv4: order_ipv4(ipv4),
                up,
            });
        }
        out
    }

    /// Parse `/proc/net/dev`: interface names in file order, the two header
    /// lines ignored, "lo" skipped (loopback is always excluded), malformed
    /// lines tolerated.
    pub(super) fn parse_proc_net_dev(input: &str) -> Vec<String> {
        let mut names = Vec::new();
        for line in input.lines().skip(2) {
            let Some((name, _stats)) = line.split_once(':') else {
                continue; // header junk / malformed line — tolerate
            };
            let name = name.trim();
            if name.is_empty() || name.contains(char::is_whitespace) {
                continue; // never a real interface name
            }
            if name == "lo" {
                continue; // loopback is always excluded
            }
            names.push(name.to_string());
        }
        names
    }

    /// Parse `/proc/net/fib_trie` into per-interface local IPv4 addresses.
    ///
    /// Only an address node whose leaf line is `/32 host LOCAL` counts as an
    /// interface's own address (that is how the local table marks assigned
    /// unicast addresses). Attribution follows bare unindented interface
    /// name lines ("wlan0"); `Main:`/`Local:` table headers do not reset it.
    /// LOCAL entries seen BEFORE the first interface line belong to the
    /// first interface section that follows (fixture shape, where the
    /// leading table tries duplicate the first adapter's address).
    ///
    /// LIMITATION (documented deviation): real kernels (≥4.x) emit no
    /// per-interface lines in fib_trie at all, so no address can be
    /// attributed — the map then comes back empty and adapters carry an
    /// empty `ipv4` list. That is fail-open by design; Windows is the
    /// production enumeration path.
    pub(super) fn parse_fib_trie(input: &str) -> BTreeMap<String, Vec<Ipv4Addr>> {
        let mut map: BTreeMap<String, Vec<Ipv4Addr>> = BTreeMap::new();
        let mut current_iface: Option<String> = None;
        let mut node: Option<Ipv4Addr> = None;
        let mut pre_header: Vec<Ipv4Addr> = Vec::new();

        for raw in input.lines() {
            let line = raw.trim_end();
            let entry = line.trim_start();
            if entry.is_empty() {
                continue;
            }
            if entry.starts_with("|--") || entry.starts_with("+--") {
                node = trie_node_addr(entry);
            } else if entry.starts_with('/') {
                let local_host =
                    entry.starts_with("/32") && entry.contains("host") && entry.contains("LOCAL");
                if local_host {
                    if let Some(ip) = node {
                        match &current_iface {
                            Some(iface) => insert_local(&mut map, iface, ip),
                            None => pre_header.push(ip),
                        }
                    }
                }
            } else if line.starts_with(char::is_whitespace) {
                // indented non-trie line: tolerated, ignored
            } else if !entry.ends_with(':') {
                // unindented bare name = interface section header
                let iface = entry.to_string();
                if current_iface.is_none() {
                    let name = iface.clone();
                    for ip in pre_header.drain(..) {
                        insert_local(&mut map, &name, ip);
                    }
                }
                current_iface = Some(iface);
            }
            // remaining case: unindented line ending in ':' = table header
            // ("Main:" / "Local:") — nothing to do
        }
        map
    }

    /// The address of a trie node line like `|-- 192.168.1.42` or
    /// `+-- 127.0.0.0/8 2 0 2` (trie stats after the address are stripped).
    fn trie_node_addr(entry: &str) -> Option<Ipv4Addr> {
        let rest = entry
            .strip_prefix("|--")
            .or_else(|| entry.strip_prefix("+--"))?;
        let token = rest.split_whitespace().next()?;
        token.split('/').next()?.parse().ok()
    }

    /// Record `ip` as local to `iface`, skipping loopback/unspecified
    /// addresses and duplicates (the same LOCAL entry can appear in both
    /// the Main: and Local: tries).
    fn insert_local(map: &mut BTreeMap<String, Vec<Ipv4Addr>>, iface: &str, ip: Ipv4Addr) {
        if ip.is_loopback() || ip.is_unspecified() {
            return;
        }
        let list = map.entry(iface.to_string()).or_default();
        if !list.contains(&ip) {
            list.push(ip);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn ipv4_ordering_puts_link_local_last() {
        let ordered = order_ipv4(vec![
            v4(169, 254, 5, 9),
            v4(10, 0, 0, 5),
            v4(169, 254, 169, 254),
            v4(192, 168, 1, 42),
        ]);
        assert_eq!(
            ordered,
            vec![
                v4(10, 0, 0, 5),
                v4(192, 168, 1, 42),
                v4(169, 254, 5, 9),
                v4(169, 254, 169, 254)
            ]
        );
    }

    #[cfg(not(windows))]
    mod linux_parsers {
        use super::super::enumerate_adapters;
        use super::super::procfs::{parse_fib_trie, parse_proc_net_dev};
        use super::v4;
        use std::collections::BTreeMap;
        use std::net::{IpAddr, Ipv4Addr};

        const DEV: &str = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 3582890    2101    0    0    0     0          0         0  3582890    2101    0    0    0     0       0          0
  eth0: 2736221886 1998416 0    0    0     0          0         0  1156469299  265231    0    0    0     0       0          0
dummy0:        0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
line without a colon
   : 1 2 3
bad name: 1 2 3
 wlan0:        0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
"#;

        /// The frozen fixture from the task spec: two interfaces, wlan0
        /// owning 192.168.1.42 (from the leading Main: trie) + 169.254.5.9,
        /// eth0 owning 10.0.0.5.
        const FIB_TRIE: &str = r#"Main:
  |-- 0.0.0.0
     /0 link UNICAST
  +-- 192.168.1.0
     /24 link UNICAST
     |-- 192.168.1.42
        /32 host LOCAL
     |-- 192.168.1.255
        /32 broadcast
wlan0
  |-- 169.254.0.0
     /16 link UNICAST
     |-- 169.254.5.9
        /32 host LOCAL
eth0
  |-- 10.0.0.0
     /24 link UNICAST
     |-- 10.0.0.5
        /32 host LOCAL
"#;

        #[test]
        fn dev_parser_lists_ifaces_and_skips_loopback() {
            let names = parse_proc_net_dev(DEV);
            assert_eq!(names, vec!["eth0", "dummy0", "wlan0"]);
            assert!(!names.iter().any(|n| n == "lo"), "loopback excluded");
        }

        #[test]
        fn dev_parser_tolerates_malformed_input() {
            // empty / header-only / colon-less / no data at all: never panics
            assert!(parse_proc_net_dev("").is_empty());
            assert!(parse_proc_net_dev("Inter-|  Receive\n face |bytes\n").is_empty());
            assert!(parse_proc_net_dev("garbage\nmore garbage\nnope\n").is_empty());
            assert!(parse_proc_net_dev("only one header\n").is_empty());
        }

        #[test]
        fn fib_trie_fixture_attributes_local_addresses() {
            let map = parse_fib_trie(FIB_TRIE);
            let mut expected = BTreeMap::new();
            expected.insert(
                "wlan0".to_string(),
                vec![
                    Ipv4Addr::new(192, 168, 1, 42),
                    Ipv4Addr::new(169, 254, 5, 9),
                ],
            );
            expected.insert("eth0".to_string(), vec![Ipv4Addr::new(10, 0, 0, 5)]);
            assert_eq!(map, expected);
        }

        #[test]
        fn fib_trie_real_kernel_shape_yields_no_attribution() {
            // Real (≥4.x) fib_trie has NO interface name lines, only the
            // Main:/Local: tries — so nothing can be attributed and the
            // map must be empty rather than misattributed.
            let real = r#"Main:
  +-- 0.0.0.0/1 3 0 5
     |-- 0.0.0.0
        /0 universe UNICAST
     |-- 21.0.5.81
        /32 host LOCAL
     +-- 127.0.0.0/8 2 0 2
        +-- 127.0.0.0/31 1 0 0
           |-- 127.0.0.0
              /32 link BROADCAST
              /8 host LOCAL
           |-- 127.0.0.1
              /32 host LOCAL
Local:
  +-- 0.0.0.0/1 3 0 5
     |-- 21.0.5.81
        /32 host LOCAL
"#;
            assert!(parse_fib_trie(real).is_empty());
        }

        #[test]
        fn fib_trie_dedups_and_skips_loopback_addresses() {
            // The same LOCAL entry appears in both tries; loopback entries
            // must never be attributed to an adapter.
            let dup = r#"Main:
     |-- 192.168.1.42
        /32 host LOCAL
     |-- 127.0.0.1
        /32 host LOCAL
Local:
     |-- 192.168.1.42
        /32 host LOCAL
wlan0
     |-- 10.0.0.5
        /32 host LOCAL
"#;
            let map = parse_fib_trie(dup);
            let mut expected = BTreeMap::new();
            expected.insert(
                "wlan0".to_string(),
                vec![Ipv4Addr::new(192, 168, 1, 42), Ipv4Addr::new(10, 0, 0, 5)],
            );
            assert_eq!(map, expected);
        }

        #[test]
        fn enumerate_is_fail_safe_and_excludes_loopback() {
            // Must not panic regardless of environment; loopback never
            // appears; ids are non-empty (bindable).
            let adapters = enumerate_adapters();
            for a in &adapters {
                assert!(!a.id.is_empty());
                assert_ne!(a.id, "lo");
                assert!(!a.ipv4.iter().any(|ip| ip.is_loopback()));
                // no APIPA-first ordering violation
                let apipa = a.ipv4.iter().any(|ip| v4(169, 254, 0, 1) == *ip);
                if apipa {
                    let first_apipa = a
                        .ipv4
                        .iter()
                        .position(|ip| matches!(ip, IpAddr::V4(v) if v.is_link_local()));
                    let last_global = a
                        .ipv4
                        .iter()
                        .rposition(|ip| !matches!(ip, IpAddr::V4(v) if v.is_link_local()));
                    if let (Some(first), Some(last)) = (first_apipa, last_global) {
                        assert!(last < first, "global addresses must come first");
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    mod win {
        use super::super::ffi::{ansi_to_string, unicast_ipv4, wide_to_string};
        use std::net::Ipv4Addr;
        use windows_sys::Win32::Networking::WinSock::{
            AF_INET, IN_ADDR, IN_ADDR_0, SOCKADDR, SOCKADDR_IN, SOCKET_ADDRESS,
        };

        #[test]
        fn string_helpers_roundtrip_and_handle_null() {
            let mut wide: Vec<u16> = "Wi-Fi".encode_utf16().collect();
            wide.push(0);
            assert_eq!(wide_to_string(wide.as_mut_ptr()).as_deref(), Some("Wi-Fi"));
            assert_eq!(wide_to_string(std::ptr::null_mut()), None);

            let mut ansi = b"{4A67B0F8-}\0".to_vec();
            assert_eq!(ansi_to_string(ansi.as_mut_ptr()), "{4A67B0F8-}");
            assert_eq!(ansi_to_string(std::ptr::null_mut()), "");
        }

        #[test]
        fn unicast_ipv4_reads_sin_addr_network_order() {
            let sockaddr_in = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: u32::from_be_bytes([192, 168, 1, 42]),
                    },
                },
                sin_zero: [0; 8],
            };
            let sa = SOCKET_ADDRESS {
                lpSockaddr: &sockaddr_in as *const SOCKADDR_IN as *mut SOCKADDR,
                iSockaddrLength: std::mem::size_of::<SOCKADDR_IN>() as i32,
            };
            assert_eq!(unicast_ipv4(sa), Some(Ipv4Addr::new(192, 168, 1, 42)));
        }
    }
}
