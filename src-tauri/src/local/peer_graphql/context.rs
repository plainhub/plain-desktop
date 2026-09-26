//! Per-request context for the peer GraphQL endpoint.
//!
//! Set on every `async_graphql::Request` via `Request::data(PeerCtx)` so
//! resolvers can recover the authenticated peer without having to parse
//! HTTP headers themselves.

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::local::db::{ChatDb, DPeer};
use crate::local::graphql::context::{AppCtx, WsEvent};

/// Everything a peer resolver needs to fulfil a request.
pub struct PeerCtx {
    /// The authenticated peer that originated the request. `from_id` in
    /// incoming chat items comes from `peer.id`.
    pub peer: DPeer,
    /// Channel id taken from the `c-cid` HTTP header. Forwarded to
    /// `create_chat_item` so the resulting chat item lands in the right
    /// channel; ignored by `channelSystemMessage` (the channel id there
    /// is part of the encrypted payload).
    pub channel_id: String,
    /// Shared application context (db handle, event bus, etc.).
    pub app: Arc<AppCtx>,
}

impl PeerCtx {
    #[allow(dead_code)]
    pub fn db(&self) -> &ChatDb {
        &self.app.db
    }

    #[allow(dead_code)]
    pub fn event_tx(&self) -> &broadcast::Sender<WsEvent> {
        &self.app.event_tx
    }
}
