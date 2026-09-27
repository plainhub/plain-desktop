//! Short ID generator. Port of the Go NAS `internal/pkg/shortid`.
//!
//! 16 bytes from `rand::thread_rng` (CSPRNG) → bigint → base57 → padded to 22
//! chars. Removes ambiguous chars (0/O/1/I/l) so it can be used in URLs
//! safely. Identical distribution to a UUID v4 (we set the v4 bits so
//! legacy consumers can still detect the format if they care).
//!
//! Distinct from [`super::short_uuid`] (base36, Kotlin-derived): this one
//! is the 22-char URL-safe format used by the plain-nas API (trash item
//! ids, task ids, tag ids).

use rand::RngCore;

const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    // Tag bytes 6/8 with the UUID v4 markers so any external consumer that
    // ever tries to parse these as UUIDs still sees a valid shape. This
    // costs nothing and avoids surprise on interop.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    encode(&bytes)
}

fn encode(uuid_bytes: &[u8]) -> String {
    let mut num: Vec<u32> = vec![0]; // little-endian big-int
    for &b in uuid_bytes {
        let mut carry = b as u32;
        for digit in num.iter_mut() {
            let v = (*digit << 8) | carry;
            *digit = v % 57;
            carry = v / 57;
        }
        while carry > 0 {
            num.push(carry % 57);
            carry /= 57;
        }
    }
    // Big-endian to readable string (most significant first)
    let mut encoded = String::new();
    for d in num.iter().rev() {
        encoded.push(ALPHABET[*d as usize] as char);
    }
    if encoded.len() < 22 {
        let pad = ALPHABET[0] as char;
        let mut s = String::new();
        for _ in 0..(22 - encoded.len()) {
            s.push(pad);
        }
        s.push_str(&encoded);
        encoded = s;
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_is_22() {
        assert_eq!(new_id().len(), 22);
    }

    #[test]
    fn unique() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
    }

    #[test]
    fn no_ambiguous_chars() {
        for _ in 0..50 {
            let id = new_id();
            for c in id.chars() {
                assert!(
                    !matches!(c, '0' | 'O' | '1' | 'I' | 'l'),
                    "ambiguous char in {id}"
                );
            }
        }
    }
}
