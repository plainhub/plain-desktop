#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NoteRow {
    pub id: String,
    pub title: String,
    pub content: String,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FeedRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub logo: String,
    pub fetch_content: bool,
    pub last_sync_at: Option<String>,
    pub last_error: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FeedEntryRow {
    pub id: String,
    pub feed_id: String,
    pub title: String,
    pub url: String,
    pub image: String,
    pub description: String,
    pub author: String,
    pub content: String,
    pub raw_id: String,
    pub published_at: String,
    pub read: bool,
    pub created_at: String,
    pub updated_at: String,
}
