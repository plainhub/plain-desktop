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
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(bytes_to_hex(&[]), "");
    }

    #[test]
    fn known_values() {
        assert_eq!(bytes_to_hex(&[0x00, 0xff, 0x10, 0xab]), "00ff10ab");
    }

    #[test]
    fn sha256_of_empty() {
        // Known constant — sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        use sha2::Digest;
        let digest = sha2::Sha256::digest(b"");
        assert_eq!(
            bytes_to_hex(digest.as_slice()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
