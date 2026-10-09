pub(crate) mod host;
pub mod schema;
mod server;
mod events;
pub use server::ContentServer;

mod audio_library;

mod audio;

mod image_index;
mod image_models;

mod image_index_host;

mod file_task_media;
mod file_task_walk;
mod file_tasks;

#[cfg(feature = "http_transport")]
mod native_resources;

mod file_writes;

mod file_access;
mod file_reads;

mod file_mutation_routes;
mod services;

mod channel_delivery;
mod channel_outgoing;
mod channel_runtime;
mod chat_store_routes;
mod contact_write;
mod discovery_advertisement;
mod guest_graphql;
mod mdns_runtime;
mod notification_actions;
mod pairing;
pub(crate) mod pairing_timeout;
mod peer_wire;
mod provider_deletes;
mod provider_plan;
#[allow(dead_code)]
mod public_dlna;
mod public_proxy;
mod public_static;
mod request_replay;
mod sms_query;
// The public `/graphql` schema is assembled domain by domain. It only starts
// serving once every contract root exists; until then `main_graphql` still
// hands the document to Kotlin and nothing under `public_schema` is
// reachable — drop these allows in the same commit that flips that over.
mod phone_geo;
#[allow(dead_code)]
mod public_audio;
#[allow(dead_code)]
mod public_calls;
#[allow(dead_code)]
mod public_chat;
#[allow(dead_code)]
mod public_clipboard;
#[allow(dead_code)]
mod public_contact_types;
#[allow(dead_code)]
mod public_contacts;
#[allow(dead_code)]
mod public_db;
#[allow(dead_code)]
mod public_device;
#[allow(dead_code)]
mod public_facts;
#[allow(dead_code)]
mod public_feeds;
#[allow(dead_code)]
mod public_file_ops;
#[allow(dead_code)]
mod public_files;
#[allow(dead_code)]
mod public_gate;
#[allow(dead_code)]
mod public_image_index;
#[allow(dead_code)]
#[allow(dead_code)]
mod public_media;
#[allow(dead_code)]
mod public_notes;
#[allow(dead_code)]
mod public_notifications;
#[allow(dead_code)]
mod public_packages;
#[allow(dead_code)]
mod public_peers;
#[allow(dead_code)]
mod public_prefs;
#[allow(dead_code)]
mod public_schema;
#[allow(dead_code)]
mod public_screen_mirror;
#[allow(dead_code)]
mod public_sms;
#[allow(dead_code)]
mod public_tags;
mod public_upload;
mod public_zip;
mod sessions;
mod sms_send;
mod sms_state;
mod system_permissions;
mod system_providers;
mod ws_login;

mod chat_delivery;
mod chat_service;

mod nearby_http;
#[cfg(feature = "http_transport")]
mod nearby_public;
mod peer_transport;
mod prewarm;

mod attachment_imports;

mod download_queue;

mod link_preview;

mod media_buckets;

mod main_graphql;

mod peer_address;
mod peer_graphql;
mod peer_lan;
mod peer_query;
mod shared_batch;
mod shared_client;
mod shared_download;
mod shared_zip;

mod pairing_runtime;

mod nearby_devices;

mod ble_pairing;

mod peer_actions;
mod peer_status;

mod status_outgoing;
mod status_socket;

mod peer_sdk;

mod peer_download;

mod chat_actions;

mod ble_http;
pub use crate::ble as ble_wire;
mod peer_files;

#[cfg(test)]
#[path = "../../tests/unit/content_api/chat_flow.rs"]
mod chat_flow;

mod thumbnails;

mod main_ws;

mod mobile_files;

mod cast_runtime;

mod mms_send;

mod dlna_sender_playback;
mod dlna_sender_runtime;
mod dlna_sender_scan;

#[cfg(feature = "http_transport")]
mod public_cors;

#[cfg(feature = "http_transport")]
mod tls_identity;

mod preferences;

mod image_search;

#[cfg(feature = "http_transport")]
mod public_lifecycle;
