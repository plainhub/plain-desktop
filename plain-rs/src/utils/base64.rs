//! Base64 (RFC 4648) in both alphabets.
//!
//! Two alphabets exist because two kinds of payload travel through the app:
//! `+/` with padding for bytes (tokens, keys, image blobs) and `-_` for
//! values that go into URLs. Decoding accepts either — a string containing
//! `-`/`_` is URL-safe, one containing `+`/`/` is standard, and one with
//! neither decodes the same either way. Mixing both in one string is an
//! error rather than a guess.

/// A rejected string: the first bad character and what it was.
#[derive(Debug, PartialEq, Eq)]
pub struct Invalid {
    pub offset: usize,
    pub found: char,
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid base64 character {:?} at {}",
            self.found, self.offset
        )
    }
}

impl std::error::Error for Invalid {}

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode(bytes: &[u8], alphabet: &[u8; 64]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(alphabet[b0 >> 2] as char);
        out.push(alphabet[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 {
            alphabet[((b1 & 0xf) << 2) | (b2 >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            alphabet[b2 & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Standard alphabet (`+` / `/`), padded with `=`.
pub fn base64_encode(bytes: &[u8]) -> String {
    encode(bytes, STANDARD)
}

/// URL-safe alphabet (`-` / `_`), padded with `=`.
pub fn base64_encode_url_safe(bytes: &[u8]) -> String {
    encode(bytes, URL_SAFE)
}

/// Decodes either alphabet, with or without `=` padding.
///
/// Padding is optional because plenty of producers drop it. A length of
/// `4n + 1` can never be valid and is rejected.
pub fn base64_decode_checked(input: &str) -> Result<Vec<u8>, Invalid> {
    let body = input.trim_end_matches('=');
    if let Some(offset) = body.find('=') {
        // Padding only ever belongs at the end.
        return Err(Invalid { offset, found: '=' });
    }
    let mut symbols: Vec<u8> = Vec::with_capacity(body.len());
    let mut tail: Option<bool> = None;
    for (offset, c) in body.char_indices() {
        let value = match c {
            'A'..='Z' => c as u8 - b'A',
            'a'..='z' => c as u8 - b'a' + 26,
            '0'..='9' => c as u8 - b'0' + 52,
            '+' | '/' | '-' | '_' => {
                let url_safe = matches!(c, '-' | '_');
                match tail {
                    Some(seen) if seen != url_safe => {
                        return Err(Invalid { offset, found: c });
                    }
                    Some(_) => {}
                    None => tail = Some(url_safe),
                }
                if c == '+' || c == '-' { 62 } else { 63 }
            }
            _ => return Err(Invalid { offset, found: c }),
        };
        symbols.push(value);
    }
    if symbols.len() % 4 == 1 {
        return Err(Invalid {
            offset: body.len(),
            found: '\0',
        });
    }
    let mut out = Vec::with_capacity(symbols.len() * 3 / 4);
    for chunk in symbols.chunks(4) {
        let v0 = chunk[0] as u32;
        let v1 = chunk[1] as u32;
        let v2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let v3 = chunk.get(3).copied().unwrap_or(0) as u32;
        out.push(((v0 << 2) | (v1 >> 4)) as u8);
        if chunk.len() > 2 {
            out.push((((v1 & 0xf) << 4) | (v2 >> 2)) as u8);
        }
        if chunk.len() > 3 {
            out.push((((v2 & 0x3) << 6) | v3) as u8);
        }
    }
    Ok(out)
}

/// Convenience wrapper for call sites that only need the bytes: an invalid
/// string decodes to nothing. Use [`base64_decode_checked`] when "this input
/// was garbage" has to be distinguishable from "this input was empty".
pub fn base64_decode(input: &str) -> Vec<u8> {
    base64_decode_checked(input).unwrap_or_default()
}

#[cfg(test)]
#[path = "../../tests/unit/utils/base64.rs"]
mod tests;
