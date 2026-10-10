//! Small helpers used across the DLNA module.

/// XML-escape a string for inclusion inside a SOAP body or DIDL-Lite payload.
/// Mirrors Go's `xml.EscapeText` behaviour, hand-rolled so we don't pull in
/// the heavier `quick-xml` writer machinery for such a tiny operation.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters (`< 0x20` except tab/lf/cr) are illegal in
            // XML 1.0; replace with U+FFFD so we don't produce broken XML.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {
                out.push('\u{FFFD}');
            }
            c => out.push(c),
        }
    }
    out
}

/// Truncate a string for log output, mirroring Go's `truncateForLog`. Returns
/// the first `max` bytes (note: byte-boundary, not char-boundary, matching the
/// Go behaviour which slices on bytes).
pub fn truncate_for_log(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        return s.to_string();
    }
    // Avoid splitting inside a UTF-8 codepoint.
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = String::with_capacity(cut + 16);
    out.push_str(&s[..cut]);
    out.push_str("\n...[truncated]");
    out
}

/// True for the lifetime of the process. The Go side exposes a runtime
/// switch (`dlnaDebugEnabled`) that is hard-wired to `true`; we follow the
/// same pattern — verbose logs are always on for now. Future: gate behind
/// `crate::log::Level::Debug` and the `--verbose` CLI flag.
pub fn dlna_debug_enabled() -> bool {
    true
}
pub fn dlna_debug_payload_enabled() -> bool {
    true
}

#[cfg(test)]
#[path = "../../tests/unit/dlna_sender/dlna/util.rs"]
mod tests;
