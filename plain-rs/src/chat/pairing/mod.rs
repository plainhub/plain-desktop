pub mod manager;
pub mod protocol;
pub mod security;
pub mod peer_store;
mod utils;

pub use manager::{PairingEvent, PairingEventKind, PairingManager};
pub use utils::{
    device_type_signature_value, local_ipv4_strs, now_ms, prefer_sender_ip, timestamp_ok,
};
