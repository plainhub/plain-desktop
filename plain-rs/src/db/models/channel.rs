use crate::chat::channel::messages::decode_members;
use crate::chat::enums::ChannelStatus;
use crate::db::now_iso;
use crate::utils::short_uuid::short_uuid;

#[derive(Clone, Debug, serde::Serialize)]
pub struct DChannel {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub members: String,
    pub key: String,
    pub version: i64,
    pub status: ChannelStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl DChannel {
    pub fn new(name: &str, owner: &str) -> Self {
        let now = now_iso();
        Self {
            id: short_uuid(),
            name: name.to_string(),
            owner_id: owner.to_string(),
            members: "[]".to_string(),
            key: String::new(),
            version: 1,
            status: ChannelStatus::Joined,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn joined_member_ids(&self) -> Vec<String> {
        decode_members(&self.members)
            .into_iter()
            .filter(|m| m.is_joined())
            .map(|m| m.peer_id)
            .collect()
    }

    /// Elect a leader for this channel from the online joined members.
    ///
    /// Direct translation of plain-app `DChatChannel.electLeader`:
    /// ```kotlin
    /// fun electLeader(onlinePeerIds: Set<String>, myId: String): String? {
    ///     val joined = joinedMembers()
    ///     val onlineJoined = joined.filter { it.id == myId || onlinePeerIds.contains(it.id) }
    ///     if (onlineJoined.isEmpty()) return null
    ///     val ownerPeerId = if (owner == "me") myId else owner
    ///     if (onlineJoined.any { it.id == ownerPeerId }) return ownerPeerId
    ///     return onlineJoined.minByOrNull { it.id }?.id
    /// }
    /// ```
    ///
    /// 1. Owner is preferred if online.
    /// 2. Fall back to the smallest online joined member id (including self).
    /// 3. Returns `None` if no eligible member is online.
    pub fn elect_leader(
        &self,
        online_ids: &std::collections::HashSet<String>,
        _my_id: &str,
    ) -> Option<String> {
        if online_ids.is_empty() {
            return None;
        }
        // Owner is preferred (plain-app resolves "me" sentinel → my_id;
        // in Rust the owner is stored as the real peer id).
        if online_ids.contains(&self.owner_id) {
            return Some(self.owner_id.clone());
        }
        // Fallback: smallest id among ALL online joined members.
        // Mirrors `onlineJoined.minByOrNull { it.id }?.id` — includes self.
        online_ids.iter().min().cloned()
    }
}
