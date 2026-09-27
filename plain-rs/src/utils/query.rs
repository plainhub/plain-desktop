use std::collections::HashMap;

/// Percent-decode a string. Replaces every `+` with a space and every
/// `%XX` byte triplet with the matching raw byte. Invalid sequences
/// (truncated `%` or non-hex digits) are passed through verbatim.
///
/// Hand-rolled to avoid pulling in the `percent-encoding` crate just
/// for a few call sites in the local server and HTTP proxy.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_nibble(bytes[i + 1]);
                let lo = hex_nibble(bytes[i + 2]);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h << 4) | l);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parse a `key=value&key=value` query string into a `HashMap`. Both
/// keys and values are percent-decoded (so `id=foo%20bar` becomes
/// `foo bar`, and `+` becomes a space).
pub fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for kv in q.split('&') {
        if kv.is_empty() {
            continue;
        }
        let (k, v) = match kv.split_once('=') {
            Some(pair) => pair,
            None => (kv, ""),
        };
        out.insert(percent_decode(k), percent_decode(v));
    }
    out
}

/// Look up a single query parameter from a path like `/foo?a=1&b=2`.
/// Returns the percent-decoded value, or `None` if the key is missing
/// or the path has no query string.
pub fn query_get(path: &str, key: &str) -> Option<String> {
    let (_, query) = path.split_once('?')?;
    parse_query(query).remove(key)
}

/// Percent-encode a string for use in a URL component. Mirrors plain-app's
/// `String.urlEncode()` (`lib/extensions/UrlEncode.kt`): unreserved chars
/// per RFC 3986 (`A–Z a–z 0–9 - _ . ~`) are passed through; every other
/// byte (the input is already UTF-8) is encoded as `%XX` with uppercase
/// hex. Used for `Content-Disposition` filenames and query-param values.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn percent_decode_case_insensitive_hex() {
        assert_eq!(percent_decode("%2F%2f"), "//");
    }

    #[test]
    fn percent_decode_invalid_passthrough() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%2"), "%2");
    }

    #[test]
    fn percent_decode_non_ascii() {
        assert_eq!(percent_decode("caf%C3%A9"), "café");
    }

    #[test]
    fn parse_query_basic() {
        let p = parse_query("id=foo&dl=1");
        assert_eq!(p.get("id").map(String::as_str), Some("foo"));
        assert_eq!(p.get("dl").map(String::as_str), Some("1"));
    }

    #[test]
    fn parse_query_percent_encoded() {
        let p = parse_query("id=hello%20world&x=y%26z");
        assert_eq!(p.get("id").map(String::as_str), Some("hello world"));
        assert_eq!(p.get("x").map(String::as_str), Some("y&z"));
    }

    #[test]
    fn parse_query_handles_plus() {
        let p = parse_query("id=hello+world");
        assert_eq!(p.get("id").map(String::as_str), Some("hello world"));
    }

    #[test]
    fn parse_query_skips_empty_segments() {
        let p = parse_query("&&id=foo&&");
        assert_eq!(p.len(), 1);
        assert_eq!(p.get("id").map(String::as_str), Some("foo"));
    }

    #[test]
    fn parse_query_valueless_key() {
        let p = parse_query("flag");
        assert_eq!(p.get("flag").map(String::as_str), Some(""));
    }

    #[test]
    fn query_get_basic() {
        assert_eq!(query_get("/foo?a=1&b=2", "a").as_deref(), Some("1"));
        assert_eq!(query_get("/foo?a=1&b=2", "b").as_deref(), Some("2"));
    }

    #[test]
    fn query_get_decodes_value() {
        assert_eq!(
            query_get("/x?cid=abc%20def", "cid").as_deref(),
            Some("abc def")
        );
    }

    #[test]
    fn query_get_missing_key() {
        assert!(query_get("/x?a=1", "b").is_none());
    }

    #[test]
    fn query_get_no_query_string() {
        assert!(query_get("/x", "a").is_none());
    }

    #[test]
    fn url_encode_unreserved_passthrough() {
        assert_eq!(url_encode("azAZ09-_.~"), "azAZ09-_.~");
    }

    #[test]
    fn url_encode_spaces_and_specials() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a/b"), "a%2Fb");
        assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn url_encode_non_ascii_utf8() {
        // café → caf%C3%A9 (UTF-8 bytes 0xC3 0xA9 for é)
        assert_eq!(url_encode("café"), "caf%C3%A9");
        // 中文 → %E4%B8%AD%E6%96%87
        assert_eq!(url_encode("中文"), "%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn url_encode_uppercase_hex() {
        assert_eq!(url_encode("\u{00e9}"), "%C3%A9");
    }
}
