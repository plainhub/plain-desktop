use crate::chat::events::{WS_PEER_STATUS_UPDATED, refresh_peer_key_cache};
use crate::chat::service::ChatService;
use crate::chat::transport::PeerTransport;
use serde_json::json;

impl<T: PeerTransport + 'static> ChatService<T> {
    /// Mirrors plain-app `PeerManager.deletePeer(peerId)`:
    ///   1. Delete all 1:1 chats with the peer.
    ///   2. If the peer is still a member of any local channel, demote it
    ///      to `status="CHANNEL"` with an empty shared key — the row
    ///      must remain so channel routing can still resolve it.
    ///   3. Otherwise delete the peer row outright.
    ///   4. Refresh the peer key cache.
    ///
    /// Returns `false` if the peer id is unknown, `true` otherwise.
    pub fn delete_peer(&self, id: &str) -> bool {
        use crate::chat::enums::PeerStatus;
        if self.db.get_peer_by_id(id).is_none() {
            return false;
        }
        self.db.delete_chats_by_peer(id);
        if self.db.any_channel_has_member(id) {
            self.db
                .update_peer_status_and_key(id, PeerStatus::Channel, "");
        } else {
            self.db.delete_peer(id);
        }
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.cacher.load(&self.db);
        self.emit(
            WS_PEER_STATUS_UPDATED,
            json!({ "id": id, "online": false }).to_string(),
        );
        true
    }

    /// Mirrors plain-app `PeerManager.markUnpaired(peerId)`: flips the
    /// peer's status to "UNPAIRED" and bumps `updated_at`, leaving the
    /// shared key intact so a future re-pair can reuse the stored
    /// credentials. Returns `false` if the peer id is unknown.
    pub fn unpair_peer(&self, id: &str) -> bool {
        use crate::chat::enums::PeerStatus;
        if self.db.get_peer_by_id(id).is_none() {
            return false;
        }
        self.db.update_peer_status(id, PeerStatus::Unpaired);
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.emit(
            WS_PEER_STATUS_UPDATED,
            json!({ "id": id, "online": false }).to_string(),
        );
        true
    }
}
