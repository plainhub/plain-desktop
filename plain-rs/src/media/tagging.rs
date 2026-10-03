use crate::media::image_index::{self, MediaSearchIndex, MediaSort};
use crate::utils::search_dsl;
use crate::{
    db::Db,
    enums::DataType,
    library::{LibraryError, LibraryResult, tags},
};

pub fn add_to_tags(db: &Db, kind: DataType, ids: &[String], query: &str) -> LibraryResult<()> {
    let keys = resolve_keys(db, kind, query)?;
    validate_tags(db, kind, ids)?;
    tags::add_relations(
        db,
        &ids.iter()
            .flat_map(|id| keys.iter().map(move |key| (id.clone(), key.clone())))
            .collect::<Vec<_>>(),
    )
}
pub fn update_relations(
    db: &Db,
    kind: DataType,
    key: &str,
    add: &[String],
    remove: &[String],
) -> LibraryResult<()> {
    tags::edit_relations(db, kind.kind(), key, add, remove)
}
pub fn remove_from_tags(db: &Db, kind: DataType, ids: &[String], query: &str) -> LibraryResult<()> {
    let keys = resolve_keys(db, kind, query)?;
    validate_tags(db, kind, ids)?;
    tags::remove_relations(db, &keys, ids)
}
fn validate_tags(db: &Db, kind: DataType, ids: &[String]) -> LibraryResult<()> {
    for id in ids {
        if tags::tag_by_id(db, id)?.is_none_or(|row| row.kind != kind.kind()) {
            return Err(LibraryError::Other("tag type mismatch".into()));
        }
    }
    Ok(())
}
fn resolve_media_keys(
    index: &MediaSearchIndex,
    kind: DataType,
    query: &str,
) -> LibraryResult<Vec<String>> {
    let mut keys = Vec::new();
    let mut offset = 0;
    loop {
        let rows = index
            .search(
                query,
                kind.media_type_str(),
                None,
                MediaSort::DateDesc,
                offset,
                500,
            )
            .map_err(|e| LibraryError::Other(e.to_string()))?;
        let count = rows.len();
        keys.extend(rows.into_iter().map(|row| row.uuid));
        if count < 500 {
            break;
        }
        offset += count;
    }
    Ok(keys)
}
fn parse_ids_query(query: &str) -> Option<Vec<String>> {
    let ids = search_dsl::field_value(query, "ids")?;
    Some(
        ids.split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
    )
}
fn resolve_keys(db: &Db, kind: DataType, query: &str) -> LibraryResult<Vec<String>> {
    if query.trim().is_empty() {
        return Err(LibraryError::Other("query is required".into()));
    }
    if let Some(ids) = parse_ids_query(query) {
        return Ok(ids);
    }
    match kind {
        DataType::Note => Ok(db.note_ids(query, None)?),
        DataType::FeedEntry => Ok(db
            .feed_entries_list(query, i64::MAX, 0)?
            .into_iter()
            .map(|row| row.id)
            .collect()),
        _ => resolve_media_keys(&image_index::global(), kind, query),
    }
}
