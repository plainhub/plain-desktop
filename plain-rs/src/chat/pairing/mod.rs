pub mod manager;
pub mod protocol;
mod utils;

pub use manager::{PairingEvent, PairingEventKind, PairingManager};
pub use utils::{
    device_type_signature_value, local_ipv4_strs, now_ms, prefer_sender_ip, timestamp_ok,
};
