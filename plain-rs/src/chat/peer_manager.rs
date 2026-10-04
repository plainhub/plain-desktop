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
        match crate::chat::app_file_store::chat_deletion::delete(
            &self.db,
            &self.data_dir,
            crate::chat::app_file_store::chat_deletion::Selection::PeerRecord(id),
        ) {
            Ok(0) => return false,
            Ok(_) => {}
            Err(error) => {
                log::error!("peer removal failed: {error}");
                return false;
            }
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
        match crate::db::chat_store::peers::unpair(&self.db, id) {
            Ok(true) => {}
            Ok(false) => return false,
            Err(error) => {
                log::error!("peer unpair failed: {error}");
                return false;
            }
        }
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.emit(
            WS_PEER_STATUS_UPDATED,
            json!({ "id": id, "online": false }).to_string(),
        );
        true
    }
}
