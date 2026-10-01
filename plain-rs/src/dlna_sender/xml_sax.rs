//! Tiny SAX-style XML parser tailored for the UPnP device description
//! we receive from `dlna/desc.rs`.
//!
//! Replaces the `quick-xml` crate we used to depend on. UPnP descriptors
//! are small (<8 KiB), well-formed, and use a fixed element set. We
//! need three things from the parser:
//!
//!   * `Start { name }` — push a tag name onto the caller's path stack
//!   * `End { name }`   — pop the tag name
//!   * `Text { content }` — the text content of the current leaf element
//!
//! Plus a "we hit the end of input" terminator. Self-closing tags
//! (`<foo/>`) emit a `Start` immediately followed by `End` (matches
//! `quick-xml`'s `expand_empty_elements` behaviour).
//!
//! We do NOT support:
//!   * CDATA, processing instructions, DOCTYPE, comments, entity refs
//!     beyond the five XML predefined ones (&amp; &lt; &gt; &quot; &apos;)
//!   * attribute parsing (we throw them away — no descriptor we care
//!     about looks at attributes)
//!   * encoding detection (the descriptor is always UTF-8; we treat
//!     invalid UTF-8 by replacing with U+FFFD)
//!
//! The total source is well under 200 lines.

use std::str;

/// Event emitted by [`Reader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Start { name: String },
    End { name: String },
    Text { content: String },
}

/// One-shot XML event reader over a byte slice. Constructed via
/// `Reader::new(bytes)` and consumed via `Reader::next_event()`.
pub struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
    pending_end: Option<String>,
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            pending_end: None,
        }
    }

    /// Pull the next event, or `None` at EOF.
    pub fn next_event(&mut self) -> Option<Event> {
        if let Some(name) = self.pending_end.take() {
            return Some(Event::End { name });
        }
        // Skip whitespace, comments, and processing instructions between
        // events. We don't emit anything for them.
        loop {
            // Skip leading whitespace and junk until we hit something
            // significant ('<' or end of input).
            while self.pos < self.input.len() && self.input[self.pos] != b'<' {
                // We see text *between* tags — collect and emit.
                let start = self.pos;
                while self.pos < self.input.len() && self.input[self.pos] != b'<' {
                    self.pos += 1;
                }
                let raw = &self.input[start..self.pos];
                // We always have a Text event for non-trivial text
                // (even if it's just whitespace — matches the
                // `trim_text(true)` setting in the old quick-xml config).
                if !raw.is_empty() {
                    return Some(Event::Text {
                        content: decode_entities(raw),
                    });
                }
            }
            if self.pos >= self.input.len() {
                return None;
            }
            // We are at '<'.
            debug_assert_eq!(self.input[self.pos], b'<');
            // Look at what follows.
            if self.pos + 3 < self.input.len() && &self.input[self.pos..self.pos + 4] == b"<!--" {
                // Comment — skip to "-->".
                if let Some(end) = find_subslice(&self.input[self.pos + 4..], b"-->") {
                    self.pos += 4 + end + 3;
                } else {
                    self.pos = self.input.len();
                }
                continue;
            }
            if self.pos + 8 < self.input.len() && &self.input[self.pos..self.pos + 2] == b"<?" {
                // Processing instruction — skip to "?>".
                if let Some(end) = find_subslice(&self.input[self.pos + 2..], b"?>") {
                    self.pos += 2 + end + 2;
                } else {
                    self.pos = self.input.len();
                }
                continue;
            }
            if self.pos + 8 < self.input.len()
                && &self.input[self.pos..self.pos + 9] == b"<!DOCTYPE"
            {
                // Doctype — skip to '>'. Good enough for the benign DOCTYPEs
                // UPnP descriptors sometimes carry; we don't validate.
                if let Some(end) = find_subslice(&self.input[self.pos + 9..], b">") {
                    self.pos += 9 + end + 1;
                } else {
                    self.pos = self.input.len();
                }
                continue;
            }

            // Real element: read up to '>'.
            let after_lt = self.pos + 1;
            let Some(gt_rel) = find_subslice(&self.input[after_lt..], b">") else {
                // Unterminated element — bail out.
                self.pos = self.input.len();
                return None;
            };
            let gt = after_lt + gt_rel;
            let raw_tag = &self.input[after_lt..gt];
            self.pos = gt + 1;

            // Closing tag?
            if raw_tag.first() == Some(&b'/') {
                let name = parse_tag_name(&raw_tag[1..]);
                return Some(Event::End { name });
            }
            // Self-closing? We need to emit both Start and End; we
            // return the Start here and the caller will see the End on
            // the next call. We have to do this because `next_event`
            // returns `Option<Event>`, not a Vec.
            if raw_tag.last() == Some(&b'/') {
                // We need to track this so the next call emits End.
                // We do that by stashing the name in the reader and
                // returning it as a synthetic End on the next call.
                let name = parse_tag_name(&raw_tag[..raw_tag.len() - 1]);
                self.pending_end = Some(name.clone());
                return Some(Event::Start { name });
            }
            // Open tag.
            let name = parse_tag_name(raw_tag);
            return Some(Event::Start { name });
        }
    }
}

/// `parse_tag_name` extracts the local element name (strips the prefix
/// `foo:bar` → `bar`, the way `quick-xml` + `local_name` did) and
/// discards any attributes (everything from the first whitespace).
fn parse_tag_name(raw: &[u8]) -> String {
    let end = raw
        .iter()
        .position(|&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r')
        .unwrap_or(raw.len());
    let head = &raw[..end];
    // Strip namespace prefix.
    let local = match head.iter().position(|&b| b == b':') {
        Some(i) => &head[i + 1..],
        None => head,
    };
    String::from_utf8_lossy(local).into_owned()
}

/// Decode the five predefined XML entities. We do not support numeric
/// character refs (they don't appear in UPnP descriptors).
fn decode_entities(raw: &[u8]) -> String {
    let s = match str::from_utf8(raw) {
        Ok(s) => s,
        Err(_) => return String::from_utf8_lossy(raw).into_owned(),
    };
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(rel) = find_subslice(&bytes[i..], b";") {
                let entity = &bytes[i + 1..i + rel];
                match entity {
                    b"amp" => {
                        out.push('&');
                        i += rel + 1;
                        continue;
                    }
                    b"lt" => {
                        out.push('<');
                        i += rel + 1;
                        continue;
                    }
                    b"gt" => {
                        out.push('>');
                        i += rel + 1;
                        continue;
                    }
                    b"quot" => {
                        out.push('"');
                        i += rel + 1;
                        continue;
                    }
                    b"apos" => {
                        out.push('\'');
                        i += rel + 1;
                        continue;
                    }
                    _ => {
                        // Unknown entity — pass through verbatim.
                    }
                }
            }
        }
        // Push a single char (lossy for invalid UTF-8 mid-stream).
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/dlna_sender/xml_sax.rs"]
mod tests;
