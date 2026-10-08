//! Self-contained XML pull parser.
//!
//! RSS/Atom feeds and OPML subscription lists are the consumers today. The
//! parser is deliberately lenient: publishers ship unescaped ampersands,
//! mismatched end tags and unclosed elements, and none of that should cost a
//! user their whole subscription list. Structural mistakes recover silently;
//! text is never dropped — an undefined named entity such as `&nbsp;` stays
//! verbatim instead of failing the document.

/// A start tag or empty element (`<x/>`). Names are lowercased and stripped
/// of their namespace prefix, so `content:encoded` becomes `encoded`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTag {
    pub name: String,
    /// The name exactly as written, e.g. `TrackDuration`. Needed when the tag
    /// name doubles as a wire key that has to keep its casing.
    pub raw: String,
    pub attrs: Vec<(String, String)>,
    /// True for `<x/>`; no matching end tag follows.
    pub empty: bool,
}

impl StartTag {
    /// Exact attribute name first, then the namespace-stripped name so
    /// `atom:href` still answers to `href`.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .or_else(|| self.attrs.iter().find(|(key, _)| local_name(key) == name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Start(StartTag),
    End(String),
    Text(String),
}

/// Namespace prefix removed, trimmed and lowercased.
pub fn local_name(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_lowercase()
}

/// Escapes a string for use inside an XML attribute value.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

pub struct Reader<'a> {
    input: &'a str,
    pos: usize,
    open: Vec<String>,
    pending: std::collections::VecDeque<Event>,
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            open: Vec::new(),
            pending: std::collections::VecDeque::new(),
        }
    }

    pub fn next(&mut self) -> Option<Event> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        if self.pos >= self.input.len() {
            // Anything still open closes implicitly instead of failing the
            // document; innermost first, so consumers unwind their stack.
            return self.open.pop().map(Event::End);
        }
        let tail = &self.input[self.pos..];
        if tail.starts_with("<!--") {
            let end = tail.find("-->").map_or(tail.len(), |end| end + 3);
            self.pos += end;
            return self.next();
        }
        if let Some(rest) = tail.strip_prefix("<![CDATA[") {
            let (text, consumed) = match rest.find("]]>") {
                Some(end) => (&rest[..end], end + 3),
                None => (rest, rest.len()),
            };
            self.pos += 9 + consumed;
            return Some(Event::Text(text.to_string()));
        }
        if tail.starts_with("<?") || tail.starts_with("<!") {
            let end = tail.find('>').map_or(tail.len(), |end| end + 1);
            self.pos += end;
            return self.next();
        }
        if let Some(rest) = tail.strip_prefix("</") {
            let end = rest.find('>')?;
            let name = local_name(&rest[..end]);
            self.pos += 2 + end + 1;
            self.close(&name);
            return self.next();
        }
        if tail.starts_with('<') {
            if tail[1..].starts_with(|c: char| c.is_alphabetic()) {
                return Some(self.read_start());
            }
            // A '<' that opens nothing is literal text.
            self.pos += 1;
            return Some(Event::Text("<".to_string()));
        }
        let end = tail.find('<').unwrap_or(tail.len());
        let raw = &tail[..end];
        self.pos += end;
        Some(Event::Text(decode(raw)))
    }

    fn close(&mut self, name: &str) {
        let Some(index) = self.open.iter().rposition(|open| open == name) else {
            return; // Stray end tag: nothing to close.
        };
        for closed in self.open.split_off(index).into_iter().rev() {
            self.pending.push_back(Event::End(closed));
        }
    }

    fn read_start(&mut self) -> Event {
        let input = self.input;
        let mut cursor = self.pos + 1;
        let name_end = scan_name_end(input, cursor);
        let raw = input[cursor..name_end].to_string();
        let name = local_name(&raw);
        cursor = name_end;
        let mut attrs: Vec<(String, String)> = Vec::new();
        let mut empty = false;
        loop {
            cursor = skip_whitespace(input, cursor);
            match input[cursor..].chars().next() {
                None => break,
                Some('>') => {
                    cursor += 1;
                    break;
                }
                Some('/') => {
                    empty = true;
                    cursor += 1;
                }
                Some(_) => {
                    let key_end = scan_name_end(input, cursor);
                    if key_end == cursor {
                        cursor += input[cursor..].chars().next().unwrap().len_utf8();
                        continue;
                    }
                    let key = &input[cursor..key_end];
                    cursor = skip_whitespace(input, key_end);
                    let value = if input[cursor..].starts_with('=') {
                        let (next, value) = read_value(input, skip_whitespace(input, cursor + 1));
                        cursor = next;
                        value
                    } else {
                        String::new()
                    };
                    let key = key.to_string();
                    if !attrs.iter().any(|(existing, _)| *existing == key) {
                        attrs.push((key, value));
                    }
                }
            }
        }
        self.pos = cursor;
        if !empty {
            self.open.push(name.clone());
        }
        Event::Start(StartTag {
            name,
            raw,
            attrs,
            empty,
        })
    }
}

fn skip_whitespace(input: &str, mut cursor: usize) -> usize {
    while let Some(c) = input[cursor..].chars().next() {
        if !c.is_whitespace() {
            break;
        }
        cursor += c.len_utf8();
    }
    cursor
}

fn scan_name_end(input: &str, mut cursor: usize) -> usize {
    while let Some(c) = input[cursor..].chars().next() {
        if c.is_whitespace() || matches!(c, '>' | '/' | '=') {
            break;
        }
        cursor += c.len_utf8();
    }
    cursor
}

fn read_value(input: &str, cursor: usize) -> (usize, String) {
    match input[cursor..].chars().next() {
        Some(quote @ ('\'' | '"')) => {
            let start = cursor + 1;
            let end = input[start..]
                .find(quote)
                .map_or(input.len(), |end| start + end);
            let value = decode(&input[start..end]);
            (end + 1, value)
        }
        Some(_) => {
            let mut end = cursor;
            while let Some(c) = input[end..].chars().next() {
                if c.is_whitespace() || matches!(c, '>' | '/') {
                    break;
                }
                end += c.len_utf8();
            }
            let value = decode(&input[cursor..end]);
            (end, value)
        }
        None => (cursor, String::new()),
    }
}

/// Decodes the five predefined XML entities plus numeric character
/// references. Anything else is left alone: an undefined named entity is a
/// publisher bug, not a reason to throw the document away.
pub fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        // Entity names are short; a far-away ';' belongs to ordinary text.
        match rest.find(';').filter(|end| *end <= 12) {
            Some(end) => match named(&rest[..end]) {
                Some(decoded) => {
                    out.push_str(&decoded);
                    rest = &rest[end + 1..];
                }
                None => out.push('&'),
            },
            None => out.push('&'),
        }
    }
    out.push_str(rest);
    out
}

fn named(name: &str) -> Option<String> {
    Some(match name {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ => return numeric(name),
    })
}

fn numeric(name: &str) -> Option<String> {
    let digits = name.strip_prefix('#')?;
    let value = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    char::from_u32(value).map(String::from)
}

#[cfg(test)]
#[path = "../../tests/unit/utils/xml.rs"]
mod tests;
