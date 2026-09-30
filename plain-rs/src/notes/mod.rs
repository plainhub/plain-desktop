use crate::enums::DataType;
use crate::library::db::LibraryDb;
use crate::library::db::notes_feeds::NoteRow;
use crate::library::tags;
use crate::library::{LibraryError, LibraryResult};

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn create(db: &LibraryDb, title: &str, content: &str) -> LibraryResult<NoteRow> {
    let id = uuid::Uuid::new_v4().to_string();
    Ok(db.note_save(&id, title, content, &now())?)
}

pub fn update(db: &LibraryDb, id: &str, title: &str, content: &str) -> LibraryResult<NoteRow> {
    if db.note_get(id)?.is_none() {
        return Err(LibraryError::Other(format!("Note {id} not found")));
    }
    Ok(db.note_save(id, title, content, &now())?)
}

pub fn search(db: &LibraryDb, query: &str, limit: i32, offset: i32) -> LibraryResult<Vec<NoteRow>> {
    Ok(db.notes_list(query, i64::from(limit.max(0)), i64::from(offset.max(0)))?)
}

pub fn count(db: &LibraryDb, query: &str) -> LibraryResult<i32> {
    Ok(db.notes_count(query)?)
}

pub fn get(db: &LibraryDb, id: &str) -> LibraryResult<Option<NoteRow>> {
    Ok(db.note_get(id)?)
}

fn explicit(query: &str) -> LibraryResult<()> {
    if query.trim().is_empty() {
        return Err(LibraryError::Other("query is required for bulk mutations — pass 'all:true' to explicitly target everything (API_SPEC §5)".to_string()));
    }
    Ok(())
}

pub fn trash(db: &LibraryDb, query: &str) -> LibraryResult<usize> {
    explicit(query)?;
    let ids = db.note_ids(query, Some(false))?;
    Ok(db.notes_set_deleted(&ids, true, &now())?)
}

pub fn restore(db: &LibraryDb, query: &str) -> LibraryResult<usize> {
    explicit(query)?;
    let ids = db.note_ids(query, Some(true))?;
    Ok(db.notes_set_deleted(&ids, false, &now())?)
}

pub fn delete(db: &LibraryDb, query: &str) -> LibraryResult<usize> {
    explicit(query)?;
    let ids = db.note_ids(query, Some(true))?;
    Ok(db.notes_delete(&ids)?)
}

pub fn save_feed_entries(db: &LibraryDb, query: &str) -> LibraryResult<Vec<String>> {
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

pub fn export(db: &LibraryDb, query: &str) -> LibraryResult<String> {
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
