use rusqlite::params;

use super::ChatDb;
use super::utils::{now_iso, short_id};

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

fn row_to_bookmark(row: &rusqlite::Row<'_>) -> rusqlite::Result<DBookmark> {
    Ok(DBookmark {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        favicon_path: row.get(3)?,
        group_id: row.get(4)?,
        pinned: row.get::<_, i32>(5)? != 0,
        click_count: row.get(6)?,
        last_clicked_at: row.get(7)?,
        sort_order: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_bookmark_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<DBookmarkGroup> {
    Ok(DBookmarkGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        collapsed: row.get::<_, i32>(2)? != 0,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

impl ChatDb {
    pub fn get_bookmarks(&self) -> Vec<DBookmark> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,url,title,favicon_path,group_id,pinned,click_count,last_clicked_at,sort_order,created_at,updated_at \
             FROM bookmarks ORDER BY sort_order ASC, created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], row_to_bookmark)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get_bookmark_by_id(&self, id: &str) -> Option<DBookmark> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT id,url,title,favicon_path,group_id,pinned,click_count,last_clicked_at,sort_order,created_at,updated_at \
             FROM bookmarks WHERE id=?",
            params![id],
            row_to_bookmark,
        )
        .ok()
    }

    pub fn get_bookmarks_by_group_id(&self, group_id: &str) -> Vec<DBookmark> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,url,title,favicon_path,group_id,pinned,click_count,last_clicked_at,sort_order,created_at,updated_at \
             FROM bookmarks WHERE group_id=? ORDER BY sort_order ASC, created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![group_id], row_to_bookmark)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn insert_bookmark(&self, bookmark: &DBookmark) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO bookmarks (id,url,title,favicon_path,group_id,pinned,click_count,last_clicked_at,sort_order,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                bookmark.id,
                bookmark.url,
                bookmark.title,
                bookmark.favicon_path,
                bookmark.group_id,
                bookmark.pinned as i32,
                bookmark.click_count,
                bookmark.last_clicked_at,
                bookmark.sort_order,
                bookmark.created_at,
                bookmark.updated_at
            ],
        );
    }

    pub fn update_bookmark(&self, bookmark: &DBookmark) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE bookmarks SET url=?1,title=?2,favicon_path=?3,group_id=?4,pinned=?5,click_count=?6,last_clicked_at=?7,sort_order=?8,updated_at=?9 WHERE id=?10",
            params![
                bookmark.url,
                bookmark.title,
                bookmark.favicon_path,
                bookmark.group_id,
                bookmark.pinned as i32,
                bookmark.click_count,
                bookmark.last_clicked_at,
                bookmark.sort_order,
                bookmark.updated_at,
                bookmark.id
            ],
        );
    }

    pub fn delete_bookmarks(&self, ids: &[String]) -> i32 {
        if ids.is_empty() {
            return 0;
        }
        let conn = self.0.lock().unwrap();
        let placeholders = (1..=ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("DELETE FROM bookmarks WHERE id IN ({placeholders})");
        conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
            .unwrap_or(0) as i32
    }

    pub fn get_bookmark_groups(&self) -> Vec<DBookmarkGroup> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,name,collapsed,sort_order,created_at,updated_at FROM bookmark_groups ORDER BY sort_order ASC, name ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], row_to_bookmark_group)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get_bookmark_group_by_id(&self, id: &str) -> Option<DBookmarkGroup> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT id,name,collapsed,sort_order,created_at,updated_at FROM bookmark_groups WHERE id=?",
            params![id],
            row_to_bookmark_group,
        )
        .ok()
    }

    pub fn insert_bookmark_group(&self, group: &DBookmarkGroup) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO bookmark_groups (id,name,collapsed,sort_order,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                group.id,
                group.name,
                group.collapsed as i32,
                group.sort_order,
                group.created_at,
                group.updated_at
            ],
        );
    }

    pub fn update_bookmark_group(&self, group: &DBookmarkGroup) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE bookmark_groups SET name=?1,collapsed=?2,sort_order=?3,updated_at=?4 WHERE id=?5",
            params![
                group.name,
                group.collapsed as i32,
                group.sort_order,
                group.updated_at,
                group.id
            ],
        );
    }

    pub fn delete_bookmark_group(&self, id: &str) {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        let _ = conn.execute("DELETE FROM bookmark_groups WHERE id=?", params![id]);
        let _ = conn.execute(
            "UPDATE bookmarks SET group_id='', updated_at=?1 WHERE group_id=?2",
            params![now, id],
        );
    }
}
