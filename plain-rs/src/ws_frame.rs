use crate::crypto::{xchacha_decrypt_raw, xchacha_encrypt_raw};
use crate::utils::base64::base64_decode;

pub fn encode_raw(event_name: &str, payload: &[u8]) -> Option<Vec<u8>> {
    if !valid_name(event_name) {
        return None;
    }
    let mut frame = Vec::with_capacity(event_name.len() + 1 + payload.len());
    frame.extend_from_slice(event_name.as_bytes());
    frame.push(0);
    frame.extend_from_slice(payload);
    Some(frame)
}

pub fn decode_raw(frame: &[u8]) -> Option<(&str, &[u8])> {
    let end = frame.iter().position(|&byte| byte == 0)?;
    let name = std::str::from_utf8(&frame[..end]).ok()?;
    if !valid_name(name) {
        return None;
    }
    Some((name, &frame[end + 1..]))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
}

pub fn encode(event_name: &str, payload: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    encode_raw(event_name, &xchacha_encrypt_raw(key, payload)?)
}

pub fn decode(frame: &[u8], key: &[u8]) -> Option<(String, Vec<u8>)> {
    let (name, encrypted) = decode_raw(frame)?;
    Some((name.to_owned(), xchacha_decrypt_raw(key, encrypted)?))
}

pub fn encode_with_token(event_name: &str, payload: &[u8], token_b64: &str) -> Option<Vec<u8>> {
    encode(event_name, payload, &base64_decode(token_b64))
}

pub fn decode_with_token(frame: &[u8], token_b64: &str) -> Option<(String, Vec<u8>)> {
    decode(frame, &base64_decode(token_b64))
}

#[cfg(test)]
#[path = "../tests/unit/ws_frame.rs"]
mod tests;
