/// Hex-encode a byte slice to a lowercase string (no `0x` prefix).
/// Used wherever we need to render a hash or fingerprint for display
/// or filename construction.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    const T: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(T[(b >> 4) as usize] as char);
        out.push(T[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/utils/hex.rs"]
mod tests;
