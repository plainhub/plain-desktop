use super::LibraryDb;
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value};

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

const NOTE_COLUMNS: &str = "id,title,content,deleted_at,created_at,updated_at";
const FEED_COLUMNS: &str = "id,name,url,fetch_content,created_at,updated_at";
const ENTRY_COLUMNS: &str = "id,feed_id,title,url,image,description,author,content,raw_id,published_at,created_at,updated_at";
const NOTE_TAG_KIND: i32 = 6;
const FEED_ENTRY_TAG_KIND: i32 = 7;

fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRow> {
    Ok(NoteRow {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        deleted_at: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn feed_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedRow> {
    Ok(FeedRow {
        id: row.get(0)?,
        name: row.get(1)?,
        url: row.get(2)?,
        fetch_content: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedEntryRow> {
    Ok(FeedEntryRow {
        id: row.get(0)?,
        feed_id: row.get(1)?,
        title: row.get(2)?,
        url: row.get(3)?,
        image: row.get(4)?,
        description: row.get(5)?,
        author: row.get(6)?,
        content: row.get(7)?,
        raw_id: row.get(8)?,
        published_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn query_filter(query: &str, notes: bool, force_trashed: Option<bool>) -> (String, Vec<Value>) {
    let fields = crate::utils::search_dsl::parse(query);
    let mut clauses = Vec::new();
    let mut values = Vec::new();
    if notes {
        let trashed = force_trashed.unwrap_or_else(|| {
            fields
                .iter()
                .any(|f| f.name == "trash" && f.value == "true")
        });
        clauses.push(
            if trashed {
                "deleted_at IS NOT NULL"
            } else {
                "deleted_at IS NULL"
            }
            .to_string(),
        );
    }
    for field in fields {
        match field.name.as_str() {
            "text" => {
                if !field.value.is_empty() {
                    clauses.push(
                        if notes {
                            "content LIKE ?"
                        } else {
                            "(title LIKE ? OR description LIKE ? OR content LIKE ?)"
                        }
                        .to_string(),
                    );
                    let pattern = format!("%{}%", field.value);
                    for _ in 0..if notes { 1 } else { 3 } {
                        values.push(Value::Text(pattern.clone()));
                    }
                }
            }
            "ids" => {
                let ids: Vec<_> = field.value.split(',').filter(|s| !s.is_empty()).collect();
                if ids.is_empty() {
                    clauses.push("0=1".to_string());
                } else {
                    clauses.push(format!("id IN ({})", vec!["?"; ids.len()].join(",")));
                    values.extend(ids.into_iter().map(|id| Value::Text(id.to_string())));
                }
            }
            "tag_id" => {
                clauses.push("id IN (SELECT key FROM tag_relations WHERE tag_id=?)".to_string());
                values.push(Value::Text(field.value));
            }
            "feed_id" if !notes => {
                clauses.push("feed_id=?".to_string());
                values.push(Value::Text(field.value));
            }
            "today" if !notes && field.value == "true" => {
                clauses.push("published_at>=?".to_string());
                values.push(Value::Text(
                    chrono::Local::now()
                        .date_naive()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_local_timezone(chrono::Local)
                        .earliest()
                        .unwrap()
                        .with_timezone(&chrono::Utc)
                        .to_rfc3339(),
                ));
            }
            "created_at"
                if !notes && ["=", "!=", ">", ">=", "<", "<="].contains(&field.op.as_str()) =>
            {
                clauses.push(format!("created_at {} ?", field.op));
                values.push(Value::Text(field.value));
            }
            _ => {}
        }
    }
    (
        if clauses.is_empty() {
            "1=1".to_string()
        } else {
            clauses.join(" AND ")
        },
        values,
    )
}

impl LibraryDb {
    pub fn notes_list(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> rusqlite::Result<Vec<NoteRow>> {
        let (where_sql, mut values) = query_filter(query, true, None);
        values.push(Value::Integer(limit.max(0)));
        values.push(Value::Integer(offset.max(0)));
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!("SELECT {NOTE_COLUMNS} FROM notes WHERE {where_sql} ORDER BY updated_at DESC LIMIT ? OFFSET ?"))?;
            stmt.query_map(params_from_iter(values), note_from_row)?.collect()
        })
    }

    pub fn notes_count(&self, query: &str) -> rusqlite::Result<i32> {
        let (where_sql, values) = query_filter(query, true, None);
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT COUNT(*) FROM notes WHERE {where_sql}"),
                params_from_iter(values),
                |r| r.get(0),
            )
        })
    }

    pub fn note_ids(
        &self,
        query: &str,
        force_trashed: Option<bool>,
    ) -> rusqlite::Result<Vec<String>> {
        let (where_sql, values) = query_filter(query, true, force_trashed);
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!("SELECT id FROM notes WHERE {where_sql}"))?;
            stmt.query_map(params_from_iter(values), |r| r.get(0))?
                .collect()
        })
    }

    pub fn note_get(&self, id: &str) -> rusqlite::Result<Option<NoteRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id=?"),
                [id],
                note_from_row,
            )
            .optional()
        })
    }

    pub fn note_save(
        &self,
        id: &str,
        title: &str,
        content: &str,
        now: &str,
    ) -> rusqlite::Result<NoteRow> {
        self.with_conn(|c| {
            c.execute("INSERT INTO notes(id,title,content,created_at,updated_at) VALUES(?1,?2,?3,?4,?4) ON CONFLICT(id) DO UPDATE SET title=excluded.title,content=excluded.content,updated_at=excluded.updated_at", params![id,title,content,now])?;
            c.query_row(&format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id=?"), [id], note_from_row)
        })
    }

    pub fn notes_set_deleted(
        &self,
        ids: &[String],
        deleted: bool,
        now: &str,
    ) -> rusqlite::Result<usize> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut count = 0;
            for id in ids {
                count += tx.execute("UPDATE notes SET deleted_at=?2,updated_at=?3 WHERE id=?1 AND ((?4=1 AND deleted_at IS NULL) OR (?4=0 AND deleted_at IS NOT NULL))", params![id, if deleted { Some(now) } else { None }, now, deleted])?;
                if deleted {
                    tx.execute("DELETE FROM tag_relations WHERE key=?1 AND tag_id IN (SELECT id FROM tags WHERE type=?2)", params![id, NOTE_TAG_KIND])?;
                }
            }
            tx.commit()?;
            Ok(count)
        })
    }

    pub fn notes_delete(&self, ids: &[String]) -> rusqlite::Result<usize> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut count = 0;
            for id in ids {
                count += tx.execute(
                    "DELETE FROM notes WHERE id=? AND deleted_at IS NOT NULL",
                    [id],
                )?;
                tx.execute("DELETE FROM tag_relations WHERE key=?1 AND tag_id IN (SELECT id FROM tags WHERE type=?2)", params![id, NOTE_TAG_KIND])?;
            }
            tx.commit()?;
            Ok(count)
        })
    }

    pub fn feeds_list(&self) -> rusqlite::Result<Vec<FeedRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {FEED_COLUMNS} FROM feeds ORDER BY name COLLATE NOCASE"
            ))?;
            stmt.query_map([], feed_from_row)?.collect()
        })
    }

    pub fn feed_get(&self, id: &str) -> rusqlite::Result<Option<FeedRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {FEED_COLUMNS} FROM feeds WHERE id=?"),
                [id],
                feed_from_row,
            )
            .optional()
        })
    }

    pub fn feed_get_by_url(&self, url: &str) -> rusqlite::Result<Option<FeedRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {FEED_COLUMNS} FROM feeds WHERE url=?"),
                [url],
                feed_from_row,
            )
            .optional()
        })
    }

    pub fn feed_save(
        &self,
        id: &str,
        name: &str,
        url: &str,
        fetch_content: bool,
        now: &str,
    ) -> rusqlite::Result<FeedRow> {
        self.with_conn(|c| {
            c.execute("INSERT INTO feeds(id,name,url,fetch_content,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,url=excluded.url,fetch_content=excluded.fetch_content,updated_at=excluded.updated_at", params![id,name,url,fetch_content,now])?;
            c.query_row(&format!("SELECT {FEED_COLUMNS} FROM feeds WHERE id=?"), [id], feed_from_row)
        })
    }

    pub fn feed_delete(&self, id: &str) -> rusqlite::Result<bool> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute("DELETE FROM tag_relations WHERE key IN (SELECT id FROM feed_entries WHERE feed_id=?1) AND tag_id IN (SELECT id FROM tags WHERE type=?2)", params![id, FEED_ENTRY_TAG_KIND])?;
            tx.execute("DELETE FROM feed_entries WHERE feed_id=?", [id])?;
            let count = tx.execute("DELETE FROM feeds WHERE id=?", [id])?;
            tx.commit()?;
            Ok(count > 0)
        })
    }

    pub fn feed_entry_get(&self, id: &str) -> rusqlite::Result<Option<FeedEntryRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {ENTRY_COLUMNS} FROM feed_entries WHERE id=?"),
                [id],
                entry_from_row,
            )
            .optional()
        })
    }

    pub fn feed_entries_list(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> rusqlite::Result<Vec<FeedEntryRow>> {
        let (where_sql, mut values) = query_filter(query, false, None);
        values.push(Value::Integer(limit.max(0)));
        values.push(Value::Integer(offset.max(0)));
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!("SELECT {ENTRY_COLUMNS} FROM feed_entries WHERE {where_sql} ORDER BY published_at DESC LIMIT ? OFFSET ?"))?;
            stmt.query_map(params_from_iter(values), entry_from_row)?.collect()
        })
    }

    pub fn feed_entry_count(&self, query: &str) -> rusqlite::Result<i32> {
        let (where_sql, values) = query_filter(query, false, None);
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT COUNT(*) FROM feed_entries WHERE {where_sql}"),
                params_from_iter(values),
                |r| r.get(0),
            )
        })
    }

    pub fn feed_entry_counts(&self) -> rusqlite::Result<Vec<(String, i32)>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare("SELECT feeds.id, COUNT(feed_entries.id) FROM feeds LEFT JOIN feed_entries ON feed_entries.feed_id=feeds.id GROUP BY feeds.id ORDER BY feeds.name COLLATE NOCASE")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        })
    }

    pub fn feed_entries_insert(
        &self,
        entries: &[FeedEntryRow],
    ) -> rusqlite::Result<Vec<FeedEntryRow>> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut inserted = Vec::new();
            for entry in entries {
                let count = tx.execute("INSERT OR IGNORE INTO feed_entries(id,feed_id,title,url,image,description,author,content,raw_id,published_at,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![entry.id,entry.feed_id,entry.title,entry.url,entry.image,entry.description,entry.author,entry.content,entry.raw_id,entry.published_at,entry.created_at,entry.updated_at])?;
                if count > 0 { inserted.push(entry.clone()); }
            }
            tx.commit()?;
            Ok(inserted)
        })
    }

    pub fn feed_entry_set_content(
        &self,
        id: &str,
        content: &str,
        now: &str,
    ) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "UPDATE feed_entries SET content=?,updated_at=? WHERE id=?",
                params![content, now, id],
            )
            .map(|_| ())
        })
    }

    pub fn feed_entries_delete(&self, query: &str) -> rusqlite::Result<usize> {
        let (where_sql, values) = query_filter(query, false, None);
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut stmt = tx.prepare(&format!("SELECT id FROM feed_entries WHERE {where_sql}"))?;
            let ids: Vec<String> = stmt
                .query_map(params_from_iter(values), |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            drop(stmt);
            for id in &ids {
                tx.execute("DELETE FROM tag_relations WHERE key=?1 AND tag_id IN (SELECT id FROM tags WHERE type=?2)", params![id, FEED_ENTRY_TAG_KIND])?;
                tx.execute("DELETE FROM feed_entries WHERE id=?", [id])?;
            }
            tx.commit()?;
            Ok(ids.len())
        })
    }
}
