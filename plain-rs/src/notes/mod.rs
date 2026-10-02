use crate::db::Db;
use crate::db::notes_feeds::NoteRow;
use crate::enums::DataType;
use crate::library::tags;
use crate::library::{LibraryError, LibraryResult};

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn create(db: &Db, title: &str, content: &str) -> LibraryResult<NoteRow> {
    let id = uuid::Uuid::new_v4().to_string();
    Ok(db.note_save(&id, &resolved_title(title, content), content, &now())?)
}

pub fn update(db: &Db, id: &str, title: &str, content: &str) -> LibraryResult<NoteRow> {
    if db.note_get(id)?.is_none() {
        return Err(LibraryError::Other(format!("Note {id} not found")));
    }
    Ok(db.note_save(id, &resolved_title(title, content), content, &now())?)
}

pub fn search(db: &Db, query: &str, limit: i32, offset: i32) -> LibraryResult<Vec<NoteRow>> {
    Ok(db.notes_list(query, i64::from(limit.max(0)), i64::from(offset.max(0)))?)
}

pub fn count(db: &Db, query: &str) -> LibraryResult<i32> {
    Ok(db.notes_count(query)?)
}

pub fn get(db: &Db, id: &str) -> LibraryResult<Option<NoteRow>> {
    Ok(db.note_get(id)?)
}

fn explicit(query: &str) -> LibraryResult<()> {
    if query.trim().is_empty() {
        return Err(LibraryError::Other("query is required for bulk mutations — pass 'all:true' to explicitly target everything (API_SPEC §5)".to_string()));
    }
    Ok(())
}

pub fn trash(db: &Db, query: &str) -> LibraryResult<usize> {
    explicit(query)?;
    let ids = db.note_ids(query, Some(false))?;
    Ok(db.notes_set_deleted(&ids, true, &now())?)
}

pub fn restore(db: &Db, query: &str) -> LibraryResult<usize> {
    explicit(query)?;
    let ids = db.note_ids(query, Some(true))?;
    Ok(db.notes_set_deleted(&ids, false, &now())?)
}

pub fn delete(db: &Db, query: &str) -> LibraryResult<usize> {
    explicit(query)?;
    let ids = db.note_ids(query, Some(true))?;
    Ok(db.notes_delete(&ids)?)
}

pub fn save_feed_entries(db: &Db, query: &str) -> LibraryResult<Vec<String>> {
    explicit(query)?;
    let entries = db.feed_entries_list(query, i64::MAX, 0)?;
    let mut ids = Vec::with_capacity(entries.len());
    for entry in entries {
        let content = format!(
            "# {}\n\n{}",
            entry.title,
            if entry.content.is_empty() {
                &entry.description
            } else {
                &entry.content
            }
        );
        db.note_save(&entry.id, &entry.title, &content, &now())?;
        ids.push(entry.id);
    }
    Ok(ids)
}

pub fn export(db: &Db, query: &str) -> LibraryResult<String> {
    let notes = db.notes_list(query, i64::MAX, 0)?;
    let values: Vec<_> = notes
        .into_iter()
        .map(|note| {
            let tag_values: Vec<_> = tags::tags_for_key_of_kind(
                db,
                &note.id,
                DataType::Note.kind(),
            )
            .into_iter()
            .map(|tag| serde_json::json!({ "id": tag.id, "name": tag.name, "count": tag.count }))
            .collect();
            serde_json::json!({ "id": note.id, "title": note.title, "content": note.content,
            "createdAt": note.created_at, "updatedAt": note.updated_at, "tags": tag_values })
        })
        .collect();
    Ok(serde_json::to_string(&values).map_err(|e| LibraryError::Other(e.to_string()))?)
}

#[cfg(test)]
#[path = "../../tests/unit/notes/mod.rs"]
mod tests;

pub fn save(db: &Db, id: &str, title: &str, content: &str) -> LibraryResult<NoteRow> {
    if id.is_empty() {
        return Err(crate::library::LibraryError::Other(
            "note id is required".into(),
        ));
    }
    Ok(db.note_save(id, &resolved_title(title, content), content, &now())?)
}

pub fn markdown_title(content: &str) -> String {
    static IMAGES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let images = IMAGES.get_or_init(|| {
        regex::Regex::new(r"(?i)!\[.*?\]\(.*?\)|!\[.*?\]\[.*?\]|<img.*?>").unwrap()
    });
    for line in content.lines() {
        if let Some(title) = line.trim().strip_prefix("# ") {
            return images.replace_all(title.trim(), "🖼").into_owned();
        }
    }
    images
        .replace_all(content, "🖼")
        .replace('\n', "")
        .trim()
        .chars()
        .take(50)
        .collect()
}
fn resolved_title<'a>(title: &'a str, content: &str) -> std::borrow::Cow<'a, str> {
    if title.is_empty() {
        markdown_title(content).into()
    } else {
        title.into()
    }
}
