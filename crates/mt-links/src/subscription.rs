//! Subscription bodies: whole-body base64 or plain one-link-per-line
//! (`parse_subscription`), plus network fetching behind the default
//! `fetch` feature (`fetch_subscription`).
//!
//! `parse_subscription` is pure and never touches the network. Failure
//! lines carry `line N:` positions of the DECODED text plus the parse
//! error, which is credential-safe by construction (link.rs never echoes
//! uuid/password/pbk values) — so failure lines are safe to paste in bug
//! reports (D9).

use std::borrow::Cow;

use crate::link::parse_share_link;
#[cfg(feature = "fetch")]
use crate::LinkError;
use crate::Profile;

/// Parse a subscription body into (profiles, human-readable failure lines).
pub fn parse_subscription(body: &str) -> (Vec<Profile>, Vec<String>) {
    let text = decode_maybe_base64(body);
    let mut profiles = Vec::new();
    let mut failures = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_share_link(line) {
            Ok(profile) => profiles.push(profile),
            Err(e) => failures.push(format!("line {}: {e}", idx + 1)),
        }
    }
    if profiles.is_empty() && failures.is_empty() {
        failures.push("subscription body is empty".to_string());
    }
    (profiles, failures)
}

/// Whole-body base64 detection (lib.rs NOTE #9): a base64 string can never
/// contain the `:` of a scheme (plain link lines always do), so the probe
/// is unambiguous; all ASCII whitespace is stripped before the decode
/// attempt because providers hard-wrap blobs. Fails safe: if the decode
/// yields no `://` either, the original body is used as plain lines (every
/// line then fails visibly — never silently swallowed).
fn decode_maybe_base64(body: &str) -> Cow<'_, str> {
    let trimmed = body.trim();
    if trimmed.contains("://") || trimmed.is_empty() {
        return Cow::Borrowed(trimmed);
    }
    let compact: String = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if let Some(bytes) = crate::b64::decode_any(&compact) {
        let decoded = String::from_utf8_lossy(&bytes);
        if decoded.contains("://") {
            return Cow::Owned(decoded.into_owned());
        }
    }
    Cow::Borrowed(trimmed)
}

// ---------------------------------------------------------------------------
// network fetch (feature `fetch`, default on)
// ---------------------------------------------------------------------------

/// Fetch a subscription URL and parse its body.
///
/// Async on purpose (global rule 7: tokio everywhere, a library must not
/// create its own runtime — `reqwest::blocking` would spin one internally;
/// lib.rs NOTE #10). 20 s total timeout; non-2xx statuses are errors;
/// error messages never contain the URL because subscription addresses can
/// embed access tokens (D9) — `reqwest::Error::without_url()` strips it
/// before the error is wrapped into the frozen `LinkError::Io` variant
/// (`LinkError` is frozen, so network failures map to `Io`).
#[cfg(feature = "fetch")]
pub async fn fetch_subscription(url: &str) -> Result<(Vec<Profile>, Vec<String>), LinkError> {
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| {
            LinkError::Io(std::io::Error::other(format!(
                "subscription client error: {e}"
            )))
        })?;
    let resp = client.get(url).send().await.map_err(net_error)?;
    let status = resp.status();
    tracing::debug!(%status, "subscription fetched");
    if !status.is_success() {
        // The status code is not a credential; the URL is not included.
        return Err(LinkError::Io(std::io::Error::other(format!(
            "subscription server returned HTTP {status}"
        ))));
    }
    let body = resp.text().await.map_err(net_error)?;
    Ok(parse_subscription(&body))
}

/// Network failures never surface the URL (D9): subscription addresses can
/// embed access tokens.
#[cfg(feature = "fetch")]
fn net_error(e: reqwest::Error) -> LinkError {
    LinkError::Io(std::io::Error::other(format!(
        "subscription fetch failed: {}",
        e.without_url()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic CONTRACTS-S6.md vectors only (RFC 5737 / dummy).
    const VLESS: &str = "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee@203.0.113.10:443?security=reality&sni=tunnel.example.org&fp=chrome&pbk=0123456789_-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde&sid=0123abcd&type=tcp&flow=xtls-rprx-vision#synthetic-reality";
    const SS: &str = "ss://YWVzLTI1Ni1nY206c3ludGhldGlj@203.0.113.10:8388#synthetic-ss";
    const TROJAN: &str =
        "trojan://synthetic-password@203.0.113.10:443?sni=tunnel.example.org#synthetic-trojan";
    const VMESS_B64: &str = "eyJ2IjoiMiIsInBzIjoic3ludGhldGljIiwiYWRkIjoiMjAzLjAuMTEzLjEwIiwicG9ydCI6IjQ0MyIsImlkIjoiYWFhYWFhYWEtYmJiYi00Y2NjLThkZGQtZWVlZWVlZWVlZWVlIiwiYWlkIjoiMCIsIm5ldCI6InRjcCIsInR5cGUiOiJub25lIiwiaG9zdCI6IiIsInBhdGgiOiIiLCJ0bHMiOiJ0bHMiLCJzbmkiOiJ0dW5uZWwuZXhhbXBsZS5vcmciLCJzY3kiOiJhdXRvIn0";

    fn plain_body() -> String {
        format!("{VLESS}\n\n{SS}\r\n{TROJAN}\nnot-a-link\n")
    }

    fn base64_body(lines: &[&str]) -> String {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        STANDARD.encode(lines.join("\n"))
    }

    #[test]
    fn parses_plain_lines_with_failures_numbered() {
        let (profiles, failures) = parse_subscription(&plain_body());
        assert_eq!(profiles.len(), 3, "vless+ss+trojan parse");
        assert_eq!(profiles[0].protocol, crate::Protocol::Vless);
        assert_eq!(profiles[1].protocol, crate::Protocol::Shadowsocks);
        assert_eq!(profiles[2].protocol, crate::Protocol::Trojan);
        assert_eq!(failures.len(), 1, "one bad line");
        assert!(
            failures[0].starts_with("line 5:"),
            "numbered failure: {}",
            failures[0]
        );
    }

    #[test]
    fn parses_whole_body_base64() {
        let body = base64_body(&[VLESS, SS, &format!("vmess://{VMESS_B64}")]);
        let (profiles, failures) = parse_subscription(&body);
        assert!(failures.is_empty(), "failures: {failures:?}");
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[2].protocol, crate::Protocol::Vmess);
    }

    #[test]
    fn parses_hard_wrapped_base64_with_whitespace() {
        let body = base64_body(&[VLESS, TROJAN]);
        // Split the blob across two "lines" like a hard-wrapped provider.
        let (a, b) = body.split_at(body.len() / 2);
        let (profiles, failures) = parse_subscription(&format!("{a}\n{b}\n"));
        assert!(failures.is_empty(), "failures: {failures:?}");
        assert_eq!(profiles.len(), 2);
    }

    #[test]
    fn garbage_body_fails_visibly_without_echo() {
        let (profiles, failures) = parse_subscription("hello world\nsecond line\n");
        assert!(profiles.is_empty());
        assert_eq!(failures.len(), 2);
        assert!(failures[0].starts_with("line 1:"), "failures: {failures:?}");
    }

    #[test]
    fn empty_body_reports_one_failure() {
        let (profiles, failures) = parse_subscription("   \n\t\n");
        assert!(profiles.is_empty());
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("empty"));
    }

    #[test]
    fn failure_lines_never_echo_credentials() {
        // A truncated uuid must not survive into the failure line.
        let bad =
            "vless://aaaaaaaa-bbbb-4ccc-8ddd-eeee@203.0.113.10:443?security=reality&type=tcp#x";
        let (profiles, failures) = parse_subscription(bad);
        assert!(profiles.is_empty());
        assert_eq!(failures.len(), 1);
        let msg = &failures[0];
        assert!(
            !msg.contains("aaaaaaaa-bbbb-4ccc-8ddd-eeee@"),
            "leaked: {msg}"
        );
        assert!(!msg.contains(bad), "input leaked: {msg}");
        assert!(msg.contains("28 chars"), "length only: {msg}");
    }

    #[cfg(feature = "fetch")]
    #[tokio::test]
    async fn fetch_invalid_url_fails_offline_without_echo() {
        // URL parsing fails client-side: no packet ever leaves the process,
        // so this test is offline-safe.
        let err = fetch_subscription("not a url at all").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("subscription fetch failed"), "msg: {msg}");
        assert!(!msg.contains("not a url at all"), "url echoed: {msg}");
    }
}
