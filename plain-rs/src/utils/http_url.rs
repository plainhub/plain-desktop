//! Tiny HTTP URL parser tailored for the small set of URL operations the
//! Plain* projects actually do. Ported from plain-nas `src/http_url.rs`
//! (which replaced the `url` crate there).
//!
//! Why not `url`?
//! --------------
//! The `url` crate is a 5000+-line WHATWG-URL-compliant parser. We do
//! not need WHATWG-URL compliance — we need to:
//!
//!   * recognise an absolute `http://` / `https://` URL,
//!   * pull out the host (and optional port), the path, the scheme, and
//!     one specific query parameter (`id=...`).
//!
//! `url` pulls in 15+ extra crates (transitively) for a feature set we
//! use 5% of.
//!
//! This module implements the subset we need (~120 lines of std):
//!   * `parse_http_url` returns a struct with the pieces the call-sites
//!     look at. Returns `None` on parse failure (caller decides what to
//!     do — typically pass the input through unchanged).
//!   * the parser is permissive on the host and path but strict on the
//!     scheme (must be `http` or `https`).
//!   * the parser is case-insensitive on the scheme and host.
//!   * we do NOT implement IDN, percent-encoding normalisation, or path
//!     resolution beyond dot-segment collapsing in `join`.
//!
//! This is deliberately not a general-purpose URL library.

use super::query::percent_decode;

/// Subset of a URL that the call-sites actually use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHttpUrl {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    /// The full query string, e.g. `id=abc&x=1`. Empty string if none.
    pub query: String,
}

impl ParsedHttpUrl {
    /// Return the first value of the query parameter `key`, or `None`.
    /// The match is case-sensitive (HTTP query keys are case-sensitive).
    /// Both keys and values are percent-decoded.
    pub fn query_param(&self, key: &str) -> Option<String> {
        parse_query_pairs(&self.query)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
}

/// Minimal `application/x-www-form-urlencoded` parser. Splits on `&`
/// then on `=`, percent-decoding both sides. Unlike `parse_query` this
/// preserves duplicate-key order (first match wins in `query_param`).
fn parse_query_pairs(query: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        out.push((percent_decode(k), percent_decode(v)));
    }
    out
}

/// Resolve a `reference` (which may be relative or absolute) against a
/// `base` URL. Mirrors the relevant subset of `url::Url::join`:
///
///   * If `reference` parses as an absolute http(s) URL, return it unchanged.
///   * Otherwise, treat `reference` as a path-absolute (`/foo`) or
///     path-relative (`foo`) URL and resolve it against `base`.
///   * Path-relative references resolve against the directory portion
///     of `base.path` (i.e. the last `/` is removed).
///
/// The returned string is the resolved absolute URL.
pub fn join(base: &str, reference: &str) -> Option<String> {
    let ref_trim = reference.trim();
    if ref_trim.is_empty() {
        return Some(base.to_string());
    }
    if parse_http_url(ref_trim).is_some() {
        return Some(ref_trim.to_string());
    }
    let base_url = parse_http_url(base)?;
    let ref_path: &str;
    let ref_query: &str;
    match ref_trim.find('?') {
        Some(qi) => {
            ref_path = &ref_trim[..qi];
            ref_query = &ref_trim[qi + 1..];
        }
        None => {
            ref_path = ref_trim;
            ref_query = "";
        }
    }
    let path = if ref_path.starts_with('/') {
        ref_path.to_string()
    } else {
        // Relative — resolve against the directory of base.path.
        let dir_end = base_url.path.rfind('/').unwrap_or(0);
        let mut new_path = String::with_capacity(dir_end + 1 + ref_path.len());
        new_path.push_str(&base_url.path[..=dir_end]);
        new_path.push_str(ref_path);
        // Collapse any "./" / "../" segments (minimal RFC 3986 path
        // normalisation — good enough for our use case).
        collapse_dot_segments(&mut new_path);
        new_path
    };
    let query = if ref_trim.contains('?') {
        ref_query.to_string()
    } else {
        base_url.query.clone()
    };
    let host_port = match base_url.port {
        Some(p) => {
            // Default ports for schemes (80 for http, 443 for https) are
            // dropped — the URL stays canonical.
            if (base_url.scheme == "http" && p == 80) || (base_url.scheme == "https" && p == 443) {
                base_url.host.clone()
            } else {
                format!("{}:{}", base_url.host, p)
            }
        }
        None => base_url.host.clone(),
    };
    let mut out = String::with_capacity(base_url.scheme.len() + host_port.len() + path.len() + 8);
    out.push_str(&base_url.scheme);
    out.push_str("://");
    out.push_str(&host_port);
    out.push_str(&path);
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query);
    }
    Some(out)
}

/// Minimal RFC 3986 §5.2.4 "remove_dot_segments" — enough for the
/// path joins we do; does not handle percent-encoding or case folding.
fn collapse_dot_segments(path: &mut String) {
    // Split the path on '/' and walk segments, applying the dot-segment
    // rules. We do it in place to keep the allocation count down.
    let mut segments: Vec<&str> = path.split('/').collect();
    let mut out: Vec<&str> = Vec::with_capacity(segments.len());
    for seg in segments.drain(..) {
        match seg {
            "." => {} // drop
            ".." => {
                if out.last().map(|s| *s != "").unwrap_or(false) {
                    out.pop();
                }
            }
            _ => out.push(seg),
        }
    }
    // Preserve leading '/' if the path started with one.
    let had_leading = path.starts_with('/');
    *path = out.join("/");
    if had_leading && !path.starts_with('/') {
        path.insert(0, '/');
    }
}

pub fn parse_http_url(input: &str) -> Option<ParsedHttpUrl> {
    let s = input.trim();
    // Find the "://" separator.
    let sep = s.find("://")?;
    let scheme = &s[..sep];
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let after = &s[sep + 3..];
    // Authority is everything up to the next '/' (path start), '?' (query
    // start), or '#' (fragment start) — fragment we ignore entirely.
    let auth_end = after
        .find(|c: char| c == '/' || c == '?' || c == '#')
        .unwrap_or(after.len());
    let authority = &after[..auth_end];
    if authority.is_empty() {
        return None;
    }
    let rest = &after[auth_end..];
    // Split authority into host[:port]. We intentionally don't accept
    // user-info (`user@host`) — none of our call-sites emit it.
    let (host, port) = split_host_port(authority)?;
    if host.is_empty() {
        return None;
    }
    // Now split `rest` into path + query.
    let (path, query) = match rest.find('?') {
        Some(qi) => (&rest[..qi], &rest[qi + 1..]),
        None => (rest, ""),
    };
    // Strip any trailing fragment (`#…`) from the query — we don't
    // support fragments.
    let query = match query.find('#') {
        Some(fi) => &query[..fi],
        None => query,
    };
    // Path must start with '/'.
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        return None;
    };
    Some(ParsedHttpUrl {
        scheme: scheme.to_ascii_lowercase(),
        host: host.to_ascii_lowercase(),
        port,
        path: path.to_string(),
        query: query.to_string(),
    })
}

fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
    // IPv6: [::1] or [::1]:8080
    if let Some(stripped) = authority.strip_prefix('[') {
        let rb = stripped.find(']')?;
        let host = &stripped[..rb];
        let rest = &stripped[rb + 1..];
        if rest.is_empty() {
            return Some((host, None));
        }
        if let Some(port_str) = rest.strip_prefix(':') {
            let port: u16 = port_str.parse().ok()?;
            return Some((host, Some(port)));
        }
        return None;
    }
    match authority.rfind(':') {
        // Bare host (no port, no colon).
        None => Some((authority, None)),
        Some(idx) => {
            let host = &authority[..idx];
            let port_str = &authority[idx + 1..];
            // If "port" is empty or non-numeric, treat the whole thing as
            // a hostname (some hostnames contain colons, though this is
            // rare; we keep behaviour conservative).
            if port_str.is_empty() {
                return Some((authority, None));
            }
            match port_str.parse::<u16>() {
                Ok(port) => Some((host, Some(port))),
                Err(_) => None, // Non-numeric port → malformed URL.
            }
        }
    }
}

/// Build `scheme://host[:port]path`, omitting the port when it is the
/// scheme's default (80 for http/ws, 443 for https/wss).
pub fn build_url(scheme: &str, host: &str, port: u16, path: &str) -> String {
    let omit_port = ((scheme == "http" || scheme == "ws") && port == 80)
        || ((scheme == "https" || scheme == "wss") && port == 443);
    let port_part = if omit_port {
        String::new()
    } else {
        format!(":{port}")
    };
    format!("{scheme}://{host}{port_part}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_http() {
        let u = parse_http_url("http://example.com/foo").unwrap();
        assert_eq!(u.scheme, "http");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, None);
        assert_eq!(u.path, "/foo");
        assert_eq!(u.query, "");
    }

    #[test]
    fn parse_https_with_port_and_query() {
        let u = parse_http_url("HTTPS://Example.COM:8443/fs?id=abc&x=1").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, Some(8443));
        assert_eq!(u.path, "/fs");
        assert_eq!(u.query, "id=abc&x=1");
        assert_eq!(u.query_param("id").as_deref(), Some("abc"));
        assert_eq!(u.query_param("missing"), None);
    }

    #[test]
    fn parse_rejects_non_http_scheme() {
        assert!(parse_http_url("ftp://example.com/").is_none());
        assert!(parse_http_url("file:///etc/passwd").is_none());
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(parse_http_url("not a url").is_none());
        assert!(parse_http_url("http://").is_none());
        assert!(parse_http_url("http://example.com:abc/").is_none());
    }

    #[test]
    fn parse_ipv6() {
        let u = parse_http_url("http://[::1]:8080/x").unwrap();
        assert_eq!(u.host, "::1");
        assert_eq!(u.port, Some(8080));
        assert_eq!(u.path, "/x");
    }

    #[test]
    fn query_param_first_match_wins() {
        let u = parse_http_url("http://e.com/?id=one&id=two").unwrap();
        assert_eq!(u.query_param("id").as_deref(), Some("one"));
    }

    #[test]
    fn join_absolute_reference_unchanged() {
        assert_eq!(
            join("http://e.com/a/", "http://other.com/x").as_deref(),
            Some("http://other.com/x")
        );
    }

    #[test]
    fn join_path_absolute() {
        assert_eq!(
            join("http://e.com/a/b", "/c").as_deref(),
            Some("http://e.com/c")
        );
    }

    #[test]
    fn join_relative_and_dot_segments() {
        assert_eq!(
            join("http://e.com/a/b", "../c/d").as_deref(),
            Some("http://e.com/c/d")
        );
    }

    #[test]
    fn build_url_omits_default_ports() {
        assert_eq!(
            build_url("http", "example.com", 80, ""),
            "http://example.com"
        );
        assert_eq!(
            build_url("https", "example.com", 443, ""),
            "https://example.com"
        );
        assert_eq!(
            build_url("ws", "example.com", 80, "/x"),
            "ws://example.com/x"
        );
    }

    #[test]
    fn build_url_keeps_non_default_ports() {
        assert_eq!(
            build_url("https", "203.0.113.5", 8443, "/fs?id=1"),
            "https://203.0.113.5:8443/fs?id=1"
        );
    }
}
