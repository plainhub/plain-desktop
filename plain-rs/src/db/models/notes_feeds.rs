#[derive(Clone, Debug)]
pub struct NoteRow {
    pub id: String,
    pub title: String,
    pub content: String,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct FeedRow {
    pub id: String,
    pub name: String,
    pub url: String,
    pub fetch_content: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
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
    pub created_at: String,
    pub updated_at: String,
}
