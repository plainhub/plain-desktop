use crate::db::{now_iso, short_id};

#[derive(Clone, Debug)]
pub struct DBookmark {
    pub id: String,
    pub url: String,
    pub title: String,
    pub favicon_path: String,
    pub group_id: String,
    pub pinned: bool,
    pub click_count: i32,
    pub last_clicked_at: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl DBookmark {
    pub fn new(url: &str, group_id: &str) -> Self {
        let now = now_iso();
        Self {
            id: short_id(),
            url: url.to_string(),
            title: url.to_string(),
            favicon_path: String::new(),
            group_id: group_id.to_string(),
            pinned: false,
            click_count: 0,
            last_clicked_at: None,
            sort_order: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DBookmarkGroup {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl DBookmarkGroup {
    pub fn new(name: &str) -> Self {
        let now = now_iso();
        Self {
            id: short_id(),
            name: name.to_string(),
            collapsed: false,
            sort_order: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
