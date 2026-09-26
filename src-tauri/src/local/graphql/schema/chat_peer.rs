//! Peer mutations — mirrors plain-app's
//! `web/schemas/ChatPeerGraphQL.kt` + `chat/peer/PeerManager.kt`.
//!
//! Both `deletePeer` and `unpairPeer` resolve through the same
//! `PeerManager` entry points as the Android side so the resulting
//! DB / cache state is identical regardless of which client issued
//! the call.

use async_graphql::{Context, Object};
use std::sync::Arc;

use super::super::context::AppCtx;

#[derive(Default)]
pub struct ChatPeerMutation;

#[Object]
impl ChatPeerMutation {
    /// Mirrors plain-app `PeerManager.deletePeer(peerId)`:
    ///   1. Delete all 1:1 chats with the peer (`ChatDbHelper.deleteAllChatsAsync`).
    ///   2. If the peer is still a member of any local channel, demote it
    ///      to `status="CHANNEL"` with an empty shared key — the row
    ///      must remain so channel routing can still resolve it.
    ///   3. Otherwise delete the peer row outright.
    ///   4. Refresh the peer key cache so future deliveries skip the
    ///      demoted / deleted peer.
    ///
    /// Returns `false` if the peer id is unknown, `true` otherwise.
    /// The frontend's `PeerManager.deletePeer` re-fetches the peers /
    /// latest-chats lists on success; no WS event is required.
    async fn delete_peer(&self, ctx: &Context<'_>, id: String) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .delete_peer(&id)
    }

    /// Mirrors plain-app `PeerManager.markUnpaired(peerId)` (invoked
    /// indirectly via `NearbyViewModel.unpairDevice`): flips the peer's
    /// status to "UNPAIRED" and bumps `updated_at`, leaving the shared
    /// key intact so a future re-pair can reuse the stored credentials.
    ///
    /// Returns `false` if the peer id is unknown, `true` otherwise.
    async fn unpair_peer(&self, ctx: &Context<'_>, id: String) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .unpair_peer(&id)
    }
}
