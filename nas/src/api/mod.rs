//! HTTP / WebSocket server. Mirrors the `cmd/services/api` package from the
//! Go side. All endpoints use the same paths, methods, headers and
//! body-encryption scheme as the original implementation.

pub mod auth;
pub mod chat_peer;
pub mod cors;
pub mod fs;
pub mod graphql;
pub mod media_thumb;
pub mod server;
pub mod static_files;
pub mod upload;
pub mod ws;
pub mod zip;
