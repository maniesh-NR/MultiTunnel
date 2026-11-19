//! base64 helpers for share links.
//!
//! v2rayN links mix standard and URL-safe alphabets, padded and unpadded,
//! sometimes within the same subscription (vmess blobs are typically
//! url-safe, SIP002 ss userinfo is url-safe-no-pad, some tools emit
//! standard+padded). Decoding therefore tries all four shapes; rendering
//! uses the SIP002 shape (URL-safe, no padding). No newline-tolerance here:
//! callers strip ASCII whitespace first (see `subscription`).

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;

/// Decode trying all four alphabet/padding shapes, most common first.
/// Returns `None` when the input decodes under none of them.
pub(crate) fn decode_any(input: &str) -> Option<Vec<u8>> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let engines = [URL_SAFE_NO_PAD, URL_SAFE, STANDARD_NO_PAD, STANDARD];
    for engine in engines {
        if let Ok(bytes) = engine.decode(input) {
            return Some(bytes);
        }
    }
    None
}

/// Encode in the SIP002 userinfo shape: base64url without padding.
pub(crate) fn encode_urlsafe_nopad(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_all_four_shapes() {
        let raw = b"method:password";
        let std_pad = STANDARD.encode(raw);
        let std_nopad = std_pad.trim_end_matches('=').to_string();
        let url_pad = URL_SAFE.encode(raw);
        let url_nopad = url_pad.trim_end_matches('=').to_string();
        for shape in [std_pad, std_nopad, url_pad, url_nopad] {
            assert_eq!(
                decode_any(&shape).as_deref(),
                Some(&raw[..]),
                "shape: {shape}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_invalid() {
        assert!(decode_any("").is_none());
        assert!(decode_any("!!!not-base64!!!").is_none());
        // ':' cannot appear in any base64 alphabet.
        assert!(decode_any("method:password").is_none());
    }

    #[test]
    fn urlsafe_encode_is_nopad_and_round_trips() {
        let s = "aes-256-gcm:synthetic";
        let enc = encode_urlsafe_nopad(s.as_bytes());
        assert!(!enc.contains('+') && !enc.contains('/') && !enc.contains('='));
        assert_eq!(decode_any(&enc).as_deref(), Some(s.as_bytes()));
        // The contract's ss vector userinfo must decode to the exact pair.
        assert_eq!(
            decode_any("YWVzLTI1Ni1nY206c3ludGhldGlj").as_deref(),
            Some(b"aes-256-gcm:synthetic".as_slice())
        );
    }
}
