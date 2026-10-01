//! Tag domain operations — plain-app `TagHelper` semantics over the
//! `tags` / `tag_relations` tables. Counts are always computed live
//! (subquery), so there is no stored count to keep in sync.

use crate::library::LibraryResult;
use crate::db::{Db, TagRelationRow, TagRow};
use crate::utils::shortid;

/// Create a tag for a data-type kind (the plain-app `DataType` ordinal).
pub fn create_tag(db: &Db, kind: i32, name: &str) -> LibraryResult<TagRow> {
    let tag = TagRow {
        id: shortid::new_id(),
        name: name.to_string(),
        kind,
        count: 0,
    };
    crate::db::tag::insert_tag(db, &tag);
    Ok(tag)
}

/// Rename a tag; `None` when the id does not exist.
pub fn update_tag(db: &Db, id: &str, name: &str) -> LibraryResult<Option<TagRow>> {
    if !crate::db::tag::update_tag_name(db, id, name) {
        return Ok(None);
    }
    Ok(crate::db::tag::tag_by_id(db, id))
}

/// Delete a tag together with all of its relations.
pub fn delete_tag(db: &Db, id: &str) {
    crate::db::tag::delete_tag(db, id)
}

pub fn tag_by_id(db: &Db, id: &str) -> Option<TagRow> {
    crate::db::tag::tag_by_id(db, id)
}

pub fn tags_by_type(db: &Db, kind: i32) -> Vec<TagRow> {
    crate::db::tag::tags_by_type(db, kind)
}

/// Relations for several keys filtered to one tag kind — the GraphQL
/// `tagRelations(type, keys)` shape, input key order preserved.
pub fn relations_for_keys_of_kind(
    db: &Db,
    keys: &[String],
    kind: i32,
) -> Vec<TagRelationRow> {
    crate::db::tag::relations_for_keys_of_kind(db, keys, kind)
}

/// Tag relations of one key (media id).
pub fn relations_for_key(db: &Db, key: &str) -> Vec<TagRelationRow> {
    crate::db::tag::relations_for_key(db, key)
}

/// Tags of `kind` related to `key`, in tag-id order — the `tags` field of
/// a media row.
pub fn tags_for_key_of_kind(db: &Db, key: &str, kind: i32) -> Vec<TagRow> {
    crate::db::tag::relations_for_key(db, key)
        .into_iter()
        .filter_map(|r| crate::db::tag::tag_by_id(db, &r.tag_id))
        .filter(|t| t.kind == kind)
        .collect()
}

/// Keys (media ids) currently related to `tag_id` — mirrors plain-app
/// `TagHelper.getKeysByTagId`.
pub fn keys_for_tag(db: &Db, tag_id: &str) -> Vec<String> {
    crate::db::tag::keys_for_tag(db, tag_id)
}

/// Add `(tag_id, key)` relations; empty ids are skipped and duplicates
/// ignored (plain-app `addToTags` skips ids the tag already has).
pub fn add_relations(db: &Db, rels: &[(String, String)]) {
    crate::db::tag::insert_relations(db, rels)
}

/// Remove the (tag_id × key) cross product — plain-app
/// `removeFromTags` / `updateTagRelations(removeTagIds)`.
pub fn remove_relations(db: &Db, keys: &[String], tag_ids: &[String]) {
    crate::db::tag::remove_relations(db, keys, tag_ids)
}

/// Drop every relation of the given keys (media deleted / trashed
/// cascade).
pub fn remove_relations_for_keys(db: &Db, keys: &[String]) {
    crate::db::tag::remove_relations_for_keys(db, keys)
}

/// Ensure a batch of `add_to_tags` relations skips ids the tags already
/// have — same result as [`add_relations`] (INSERT OR IGNORE), kept as a
/// named seam because plain-app dedupes before writing.
pub fn add_relations_checked(db: &Db, rels: &[(String, String)]) {
    add_relations(db, rels)
}

#[cfg(test)]
#[path = "../../tests/unit/library/tags.rs"]
mod tests;
