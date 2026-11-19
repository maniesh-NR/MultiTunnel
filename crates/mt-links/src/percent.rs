//! Percent-encoding policy for share links (see lib.rs IMPLEMENTATION
//! NOTES #3).
//!
//! Fields that carry user data are DECODED on parse (lossy) and ENCODED on
//! render, making `parse(render(p)) == p` a fixed point for any UTF-8
//! payload: remark/fragment, userinfo/identity, transport values, sni,
//! alpn items, flow, fingerprint, spider_x. Structural fields (address,
//! port, pbk, sid) are never decoded or encoded — their alphabets cannot
//! contain escapes (pbk/sid are alphabet-validated at parse), so accepted
//! values always round-trip byte-exactly.

use percent_encoding::{percent_decode_str, percent_encode, AsciiSet, CONTROLS};

/// Fragment (`#remark`): escape what would terminate the fragment or start
/// a new escape that we did not write.
const FRAGMENT: &AsciiSet = &CONTROLS.add(b'%').add(b'#');

/// Userinfo (`<uuid|password>@`): escape `@` (ends the userinfo) and
/// everything else a URL parser treats structurally inside it.
const USERINFO: &AsciiSet = &CONTROLS
    .add(b'%')
    .add(b'@')
    .add(b':')
    .add(b'/')
    .add(b'?')
    .add(b'#')
    .add(b'[')
    .add(b']');

/// Query-parameter values: escape the `&`/`=`/`#`/`+`/`?` delimiters (plus
/// space and controls). `,` is escaped so alpn items stay splittable; all
/// other non-ASCII bytes are always escaped by the encoder (an `AsciiSet`
/// can only allow ASCII bytes), which is what makes decode/encode exact.
const QUERY_VALUE: &AsciiSet = &CONTROLS
    .add(b'%')
    .add(b'&')
    .add(b'=')
    .add(b'#')
    .add(b'+')
    .add(b',')
    .add(b'?')
    .add(b' ');

/// Lossy percent-decode (invalid sequences survive as literal bytes).
pub(crate) fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

pub(crate) fn encode_fragment(s: &str) -> String {
    percent_encode(s.as_bytes(), FRAGMENT).to_string()
}

pub(crate) fn encode_userinfo(s: &str) -> String {
    percent_encode(s.as_bytes(), USERINFO).to_string()
}

pub(crate) fn encode_query_value(s: &str) -> String {
    percent_encode(s.as_bytes(), QUERY_VALUE).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_encode_is_a_fixed_point() {
        for s in [
            "plain",
            "tunnel.example.org",
            "/ws?ed=2560&x=1",
            "a b+c&d=e#f%20g",
            "隧道-01",              // multi-byte UTF-8
            "password with spaces", // userinfo-style
            "spx=/spider?a=b",
            "100%",
        ] {
            assert_eq!(decode(&encode_query_value(s)), s, "query: {s}");
            assert_eq!(decode(&encode_fragment(s)), s, "fragment: {s}");
            assert_eq!(decode(&encode_userinfo(s)), s, "userinfo: {s}");
        }
    }

    #[test]
    fn delimiters_are_escaped_in_the_right_sets() {
        assert!(encode_fragment("a#b").contains("%23"));
        assert!(encode_query_value("a&b").contains("%26"));
        assert!(encode_query_value("a=b").contains("%3D"));
        assert!(encode_userinfo("a@b").contains("%40"));
        assert_eq!(
            encode_query_value("tunnel.example.org"),
            "tunnel.example.org"
        );
        // '/' is not in the query set: it stays readable (and round-trips).
        assert_eq!(encode_query_value("/ws"), "/ws");
        assert!(encode_query_value("隧道").contains('%'));
    }

    #[test]
    fn invalid_escapes_survive_lossy() {
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("a%zz"), "a%zz");
    }
}
