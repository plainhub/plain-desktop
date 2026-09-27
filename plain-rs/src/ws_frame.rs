//! WebSocket event frame codec shared by plain-desktop and plain-nas.
//!
//! Wire format (both projects already speak this; the Go NAS original used
//! the same framing for its `wsHub`):
//!
//! ```text
//! [4-byte big-endian i32 msg_type][XChaCha20-Poly1305 encrypted JSON payload]
//! ```
//!
//! where the ciphertext is `plain_rs::xchacha_*` output, i.e.
//! `nonce (24 bytes) || ciphertext || tag (16)`. The connecting client must
//! prove possession of the key before frames flow (see each project's WS
//! handler: the first binary frame must decrypt successfully).

use crate::crypto::{xchacha_decrypt_raw, xchacha_encrypt_raw};
use crate::utils::base64::base64_decode;

/// Number of leading bytes in a frame that carry the message type.
pub const TYPE_LEN: usize = 4;

/// Encode `msg_type || xchacha(key, payload)`.
pub fn encode(msg_type: i32, payload: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    let encrypted = xchacha_encrypt_raw(key, payload)?;
    let mut msg = Vec::with_capacity(TYPE_LEN + encrypted.len());
    msg.extend_from_slice(&msg_type.to_be_bytes());
    msg.extend_from_slice(&encrypted);
    Some(msg)
}

/// Inverse of [`encode`]. Returns `(msg_type, decrypted_payload)` or `None`
/// when the frame is too short or fails authentication.
pub fn decode(frame: &[u8], key: &[u8]) -> Option<(i32, Vec<u8>)> {
    if frame.len() < TYPE_LEN + 24 {
        return None;
    }
    let msg_type = i32::from_be_bytes(frame[..TYPE_LEN].try_into().ok()?);
    let payload = xchacha_decrypt_raw(key, &frame[TYPE_LEN..])?;
    Some((msg_type, payload))
}

/// [`encode`] with a base64-encoded 32-byte key (the URL token form used by
/// the local-server ↔ browser channel).
pub fn encode_with_token(msg_type: i32, payload: &[u8], token_b64: &str) -> Option<Vec<u8>> {
    encode(msg_type, payload, &base64_decode(token_b64))
}

/// [`decode`] with a base64-encoded 32-byte key.
pub fn decode_with_token(frame: &[u8], token_b64: &str) -> Option<(i32, Vec<u8>)> {
    decode(frame, &base64_decode(token_b64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::base64::base64_encode;

    #[test]
    fn round_trip_raw_key() {
        let key = [7u8; 32];
        let frame = encode(42, b"hello", &key).unwrap();
        let (t, p) = decode(&frame, &key).unwrap();
        assert_eq!(t, 42);
        assert_eq!(p, b"hello");
    }

    #[test]
    fn round_trip_token_key() {
        let raw = [9u8; 32];
        let token = base64_encode(&raw);
        let frame = encode_with_token(3, b"{}", &token).unwrap();
        let (t, p) = decode_with_token(&frame, &token).unwrap();
        assert_eq!(t, 3);
        assert_eq!(p, b"{}");
    }

    #[test]
    fn frame_layout_is_type_plus_nonce_plus_ct() {
        let key = [1u8; 32];
        let frame = encode(1, b"x", &key).unwrap();
        // 4 type + 24 nonce + 1 payload + 16 tag
        assert_eq!(frame.len(), 4 + 24 + 1 + 16);
        assert_eq!(&frame[..4], &1i32.to_be_bytes());
    }

    #[test]
    fn decode_rejects_wrong_key_and_short_frames() {
        let frame = encode(1, b"x", &[1u8; 32]).unwrap();
        assert!(decode(&frame, &[2u8; 32]).is_none());
        assert!(decode(&frame[..10], &[1u8; 32]).is_none());
        assert!(decode(&[], &[1u8; 32]).is_none());
    }
}
