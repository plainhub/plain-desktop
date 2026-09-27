/// Generate a short, human-friendly unique ID derived from a UUID v4 string.
///
/// Mirrors the Kotlin `StringHelper.shortUUID()` used in `plain-app`:
/// take a fresh UUID v4, read the first 8 bytes of its string form as a
/// big-endian `u64`, and render it as a base-36 (`Character.MAX_RADIX`)
/// number.
pub fn short_uuid() -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let bytes = id.as_bytes();
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    let n = u64::from_be_bytes(buf);
    encode_base36(n)
}

/// Render a `u64` as an unsigned base-36 string (digits `0-9`, `a-z`).
/// Matches Java's `Long.toString(Character.MAX_RADIX)`.
fn encode_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut buf = Vec::with_capacity(13);
    while n > 0 {
        let digit = (n % 36) as u32;
        buf.push(std::char::from_digit(digit, 36).unwrap());
        n /= 36;
    }
    buf.reverse();
    buf.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_uuid_is_non_empty_and_base36() {
        let id = short_uuid();
        assert!(!id.is_empty());
        assert!(
            id.chars()
                .all(|c| c.is_ascii_digit() || (c.is_ascii_lowercase()))
        );
    }

    #[test]
    fn short_uuid_is_unique() {
        let a = short_uuid();
        let b = short_uuid();
        assert_ne!(a, b);
    }

    #[test]
    fn encode_base36_zero() {
        assert_eq!(encode_base36(0), "0");
    }

    #[test]
    fn encode_base36_matches_java() {
        // Sanity check: 36 -> "10" (matches Long.toString(36, 36) -> "10").
        assert_eq!(encode_base36(36), "10");
        assert_eq!(encode_base36(35), "z");
    }
}
