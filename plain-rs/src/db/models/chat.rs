use crate::chat::enums::ChatStatus;
use crate::db::{now_iso, short_id};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DChat {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub channel_id: String,
    pub content: String,
    pub status: ChatStatus,
    pub status_data: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DChat {
    pub fn new(from_id: &str, to_id: &str, channel_id: &str, content: &str) -> Self {
        let now = now_iso();
        Self {
            id: short_id(),
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            channel_id: channel_id.to_string(),
            content: content.to_string(),
            status: ChatStatus::Sent,
            status_data: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
