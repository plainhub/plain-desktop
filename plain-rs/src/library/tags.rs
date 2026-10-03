use crate::db::tag;
use crate::{
    db::{Db, TagRelationRow, TagRow},
    library::{LibraryError, LibraryResult},
};

pub fn create_tag(db: &Db, kind: i32, name: &str) -> LibraryResult<TagRow> {
    let row = TagRow {
        id: crate::utils::shortid::new_id(),
        name: name.into(),
        kind,
        count: 0,
    };
    tag::insert_tag(db, &row)?;
    Ok(row)
}
pub fn update_tag(db: &Db, id: &str, name: &str) -> LibraryResult<Option<TagRow>> {
    Ok(tag::rename_tag(db, id, name)?)
}
pub fn delete_tag(db: &Db, id: &str) -> LibraryResult<()> {
    Ok(tag::delete_tag(db, id)?)
}
pub fn tag_by_id(db: &Db, id: &str) -> LibraryResult<Option<TagRow>> {
    Ok(tag::tag_by_id(db, id)?)
}
pub fn tags_by_type(db: &Db, kind: i32) -> LibraryResult<Vec<TagRow>> {
    Ok(tag::tags_by_type(db, kind)?)
}
pub fn relations_for_keys_of_kind(
    db: &Db,
    keys: &[String],
    kind: i32,
) -> LibraryResult<Vec<TagRelationRow>> {
    Ok(tag::relations_for_keys_of_kind(db, keys, kind)?)
}
pub fn relations_for_key(db: &Db, key: &str) -> LibraryResult<Vec<TagRelationRow>> {
    Ok(tag::relations_for_key(db, key)?)
}
pub fn tags_for_key_of_kind(db: &Db, key: &str, kind: i32) -> LibraryResult<Vec<TagRow>> {
    Ok(tag::tags_for_key_of_kind(db, key, kind)?)
}
pub fn keys_for_tag(db: &Db, id: &str) -> LibraryResult<Vec<String>> {
    Ok(tag::keys_for_tag(db, id)?)
}
pub fn add_relations(db: &Db, relations: &[(String, String)]) -> LibraryResult<()> {
    Ok(tag::insert_relations(db, relations)?)
}
pub fn remove_relations(db: &Db, keys: &[String], ids: &[String]) -> LibraryResult<()> {
    Ok(tag::remove_relations(db, keys, ids)?)
}
pub fn remove_relations_for_keys(db: &Db, keys: &[String]) -> LibraryResult<()> {
    Ok(tag::remove_relations_for_keys(db, keys)?)
}
pub fn add_relations_checked(db: &Db, relations: &[(String, String)]) -> LibraryResult<()> {
    add_relations(db, relations)
}
pub fn edit_relations(
    db: &Db,
    kind: i32,
    key: &str,
    add: &[String],
    remove: &[String],
) -> LibraryResult<()> {
    if key.is_empty() {
        return Err(LibraryError::Other("empty tag key".into()));
    }
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        for id in add.iter().chain(remove) {
            if tag::io::tag_by_id(&tx, id)?.is_none_or(|row| row.kind != kind) {
                return Err(LibraryError::Other("tag type mismatch".into()));
            }
        }
        tag::io::insert_relations(
            &tx,
            &add.iter()
                .map(|id| (id.clone(), key.into()))
                .collect::<Vec<_>>(),
        )?;
        tag::io::remove_relations(&tx, &[key.into()], remove)?;
        tx.commit()?;
        Ok(())
    })
}
#[cfg(test)]
#[path = "../../tests/unit/library/tags.rs"]
mod tests;
