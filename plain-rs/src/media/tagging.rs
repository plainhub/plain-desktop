use crate::db::Db;
use crate::enums::DataType;
use crate::media::image_index::{self, MediaSearchIndex, MediaSort};
use crate::utils::search_dsl;

pub fn add_to_tags(db: &Db, kind: DataType, tag_ids: &[String], query: &str) {
    if tag_ids.is_empty() {
        return;
    }
    let keys = resolve_keys(db, kind, query);
    if keys.is_empty() {
        return;
    }
    let mut relations = Vec::new();
    for tag_id in tag_ids {
        let existing: std::collections::HashSet<String> =
            crate::library::tags::keys_for_tag(db, tag_id)
                .into_iter()
                .collect();
        for key in &keys {
            if !existing.contains(key) {
                relations.push((tag_id.clone(), key.clone()));
            }
        }
    }
    crate::library::tags::add_relations(db, &relations);
}

pub fn update_relations(db: &Db, key: &str, add_tag_ids: &[String], remove_tag_ids: &[String]) {
    let add = add_tag_ids
        .iter()
        .filter(|tag_id| !tag_id.is_empty())
        .map(|tag_id| (tag_id.clone(), key.to_string()))
        .collect::<Vec<_>>();
    if !add.is_empty() {
        crate::library::tags::add_relations(db, &add);
    }
    if !remove_tag_ids.is_empty() {
        crate::library::tags::remove_relations(db, &[key.to_string()], remove_tag_ids);
    }
}

pub fn remove_from_tags(db: &Db, kind: DataType, tag_ids: &[String], query: &str) {
    if tag_ids.is_empty() {
        return;
    }
    let keys = resolve_keys(db, kind, query);
    if !keys.is_empty() {
        crate::library::tags::remove_relations(db, &keys, tag_ids);
    }
}

fn resolve_media_keys(index: &MediaSearchIndex, kind: DataType, query: &str) -> Vec<String> {
    if let Some(keys) = parse_ids_query(query) {
        return keys;
    }
    match index.search(
        query,
        kind.media_type_str(),
        None,
        MediaSort::DateDesc,
        0,
        10_000,
    ) {
        Ok(rows) => rows.into_iter().map(|row| row.uuid).collect(),
        Err(error) => {
            log::error!("[tags] resolve keys failed for {query:?}: {error}");
            Vec::new()
        }
    }
}

fn parse_ids_query(query: &str) -> Option<Vec<String>> {
    let ids = search_dsl::parse(query)
        .into_iter()
        .filter(|field| field.name == "ids")
        .map(|field| field.value)
        .last()?;
    if ids.trim().is_empty() {
        return None;
    }
    Some(
        ids.split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn resolve_keys(db: &Db, kind: DataType, query: &str) -> Vec<String> {
    match kind {
        DataType::Note => db.note_ids(query, None).unwrap_or_default(),
        DataType::FeedEntry => db
            .feed_entries_list(query, i64::MAX, 0)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| entry.id)
            .collect(),
        _ => resolve_media_keys(&image_index::global(), kind, query),
    }
}
