/// Standard base64 encoder (RFC 4648, with `+` and `/`, padded with `=`).
pub fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        out.push(T[b0 >> 2] as char);
        out.push(T[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 {
            T[((b1 & 0xf) << 2) | (b2 >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[b2 & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Standard base64 decoder. Ignores `=` padding; returns an empty Vec on empty input.
pub fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u8 {
        match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    };
    let cleaned: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4 + 1);
    for chunk in cleaned.chunks(4) {
        let v0 = val(chunk[0]);
        let v1 = if chunk.len() > 1 { val(chunk[1]) } else { 0 };
        let v2 = if chunk.len() > 2 { val(chunk[2]) } else { 0 };
        let v3 = if chunk.len() > 3 { val(chunk[3]) } else { 0 };
        out.push((v0 << 2) | (v1 >> 4));
        if chunk.len() > 2 {
            out.push(((v1 & 0xf) << 4) | (v2 >> 2));
        }
        if chunk.len() > 3 {
            out.push(((v2 & 3) << 6) | v3);
        }
    }
    out
}
