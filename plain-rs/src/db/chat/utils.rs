//! Row-id + timestamp helpers for the chat tables. The timestamp
//! functions live in [`crate::utils::dbtime`] (shared with the
//! `library` feature); `short_id` is the chat-specific 8-byte-hex id.

pub use crate::utils::dbtime::{iso_from_unix_millis, now_iso, now_millis};

pub fn short_id() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 8];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/db/utils.rs"]
mod tests;
