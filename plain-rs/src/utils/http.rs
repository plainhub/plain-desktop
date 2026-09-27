/// CORS preflight headers. Same on the local server and the upstream
/// HTTP proxy — both surfaces serve the web UI, so the policy is
/// identical.
pub const CORS: &[u8] = b"access-control-allow-origin: *\r\n\
                       access-control-allow-methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
                       access-control-allow-headers: *\r\n";

/// Outcome of parsing an HTTP `Range` header against a representation size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeParse {
    /// No usable `Range` header (absent, non-`bytes` unit, or malformed) —
    /// serve the full 200 response, per RFC 7233 §3.1 ("an origin server
    /// MUST ignore a Range header field that contains a range unit it does
    /// not understand" and malformed ranges are likewise ignored).
    Full,
    /// A satisfiable byte range: inclusive `(start, end)`.
    Partial(u64, u64),
    /// Syntactically valid but unsatisfiable (start ≥ size, or a zero-length
    /// suffix) — respond 416 with `Content-Range: bytes */size`, per
    /// RFC 7233 §4.4.
    Unsatisfiable,
}

/// Parse an HTTP `Range` header (RFC 7233 §2.1) against a file size.
///
/// Supported forms:
///   * `bytes=0-499`   → first 500 bytes
///   * `bytes=500-`    → from byte 500 to end
///   * `bytes=-500`    → last 500 bytes
///
/// Multi-range requests (`bytes=0-10,20-30`) are not supported — the
/// first range is served and the rest ignored, which keeps downloads
/// working without the multipart/byteranges dance.
pub fn parse_range_header(range: &str, file_size: u64) -> RangeParse {
    use RangeParse::*;
    let Some(spec) = range.strip_prefix("bytes=") else {
        return Full;
    };
    let Some(spec) = spec.split(',').next() else {
        return Full;
    };
    let spec = spec.trim();
    let Some((start_s, end_s)) = spec.split_once('-') else {
        return Full;
    };
    let start_s = start_s.trim();
    let end_s = end_s.trim();
    if file_size == 0 {
        // Nothing can be served partially; a 200 with an empty body is the
        // least surprising response and matches the pre-tri-state behavior.
        return Full;
    }
    match (start_s.is_empty(), end_s.is_empty()) {
        (false, false) => {
            let (Ok(start), Ok(end)) = (start_s.parse::<u64>(), end_s.parse::<u64>()) else {
                return Full;
            };
            if start > end {
                // Inverted range: invalid per RFC 7233 §2.1, ignore the header.
                return Full;
            }
            if start >= file_size {
                return Unsatisfiable;
            }
            Partial(start, end.min(file_size - 1))
        }
        (false, true) => {
            let Ok(start) = start_s.parse::<u64>() else {
                return Full;
            };
            if start >= file_size {
                return Unsatisfiable;
            }
            Partial(start, file_size - 1)
        }
        (true, false) => {
            let Ok(n) = end_s.parse::<u64>() else {
                return Full;
            };
            if n == 0 {
                return Unsatisfiable;
            }
            Partial(file_size.saturating_sub(n), file_size - 1)
        }
        (true, true) => Full, // "bytes=-": malformed, ignore
    }
}

/// Build a `Content-Disposition` header value (RFC 6266 / RFC 5987).
/// `kind` is `"inline"` or `"attachment"`; the filename is URL-encoded for
/// both the legacy `filename="…"` and the `filename*=utf-8''…` forms —
/// matching plain-app exactly so non-ASCII names round-trip correctly.
pub fn content_disposition(kind: &str, filename: &str) -> String {
    let encoded = super::query::url_encode(filename);
    format!("{kind}; filename=\"{encoded}\"; filename*=utf-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use RangeParse::{Full, Partial, Unsatisfiable};

    #[test]
    fn parse_range_start_end() {
        assert_eq!(parse_range_header("bytes=0-499", 1000), Partial(0, 499));
        assert_eq!(parse_range_header("bytes=100-199", 1000), Partial(100, 199));
    }

    #[test]
    fn parse_range_open_end() {
        assert_eq!(parse_range_header("bytes=500-", 1000), Partial(500, 999));
        assert_eq!(parse_range_header("bytes=0-", 1000), Partial(0, 999));
    }

    #[test]
    fn parse_range_suffix() {
        assert_eq!(parse_range_header("bytes=-500", 1000), Partial(500, 999));
        assert_eq!(parse_range_header("bytes=-2000", 1000), Partial(0, 999));
    }

    #[test]
    fn parse_range_clamps_end_to_file_size() {
        assert_eq!(
            parse_range_header("bytes=900-2000", 1000),
            Partial(900, 999)
        );
    }

    #[test]
    fn parse_range_out_of_bounds_is_unsatisfiable() {
        // RFC 7233 §4.4: syntactically valid but beyond EOF → 416 material.
        assert_eq!(parse_range_header("bytes=1000-", 1000), Unsatisfiable);
        assert_eq!(parse_range_header("bytes=2000-3000", 1000), Unsatisfiable);
        assert_eq!(parse_range_header("bytes=-0", 1000), Unsatisfiable);
        assert_eq!(parse_range_header("bytes=999-", 1000), Partial(999, 999));
    }

    #[test]
    fn parse_range_malformed_is_full() {
        // Unknown units, unparsable numbers and inverted ranges are ignored
        // (RFC 7233 §3.1) — the caller serves the whole representation.
        assert_eq!(parse_range_header("", 1000), Full);
        assert_eq!(parse_range_header("items=0-10", 1000), Full);
        assert_eq!(parse_range_header("bytes=abc-def", 1000), Full);
        assert_eq!(parse_range_header("bytes=-", 1000), Full);
        assert_eq!(parse_range_header("bytes=5-2", 1000), Full);
        assert_eq!(parse_range_header("bytes=0-10", 0), Full);
    }

    #[test]
    fn parse_range_multi_range_serves_first() {
        assert_eq!(parse_range_header("bytes=0-10,20-30", 1000), Partial(0, 10));
    }

    #[test]
    fn content_disposition_inline_and_attachment() {
        assert_eq!(
            content_disposition("inline", "cat.jpg"),
            "inline; filename=\"cat.jpg\"; filename*=utf-8''cat.jpg"
        );
        assert_eq!(
            content_disposition("attachment", "r port.pdf"),
            "attachment; filename=\"r%20port.pdf\"; filename*=utf-8''r%20port.pdf"
        );
    }

    #[test]
    fn content_disposition_encodes_non_ascii() {
        assert_eq!(
            content_disposition("inline", "中文.pdf"),
            "inline; filename=\"%E4%B8%AD%E6%96%87.pdf\"; filename*=utf-8''%E4%B8%AD%E6%96%87.pdf"
        );
    }
}
