//! DLNA MediaRenderer receiver — engine/protocol in plain-rs
//! (`local_api::dlna`); the `commands` module is the Tauri bridge.

pub mod commands;

#[allow(unused_imports)]
pub use plain_rs::local_api::dlna::{
    http_router, is_receiver_path, receiver_engine, renderer_state, senders_contain_ip,
    soap_handler, ssdp_messages, types, xml_templates,
};
