use crate::{
    db::{Db, tag},
    library::{LibraryError, LibraryResult},
};
use rusqlite::{Connection, OptionalExtension, params};

pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub kind: i32,
    pub count: i32,
    pub created_at: String,
    pub updated_at: String,
}
pub struct RelationRecord {
    pub tag_id: String,
    pub key: String,
    pub kind: i32,
    pub created_at: String,
    pub size_bytes: i64,
    pub title: String,
}
pub struct RelationInput {
    pub tag_id: String,
    pub key: String,
    pub kind: i32,
    pub size_bytes: i64,
    pub title: String,
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TagRecord> {
    Ok(TagRecord {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        count: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}
const COLUMNS: &str =
    "id,name,type,(SELECT COUNT(*) FROM tag_relations WHERE tag_id=tags.id),created_at,updated_at";
pub fn all(db: &Db, kind: i32) -> LibraryResult<Vec<TagRecord>> {
    Ok(db.with_conn(|c| {
        c.prepare(&format!(
            "SELECT {COLUMNS} FROM tags WHERE type=? ORDER BY rowid"
        ))?
        .query_map([kind], row)?
        .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}
pub fn get(db: &Db, id: &str) -> LibraryResult<Option<TagRecord>> {
    Ok(db.with_conn(|c| {
        c.query_row(&format!("SELECT {COLUMNS} FROM tags WHERE id=?"), [id], row)
            .optional()
    })?)
}
pub fn relations(db: &Db, kind: i32, keys: &[String]) -> LibraryResult<Vec<RelationRecord>> {
    Ok(db.with_conn(|c|c.prepare("SELECT tag_id,key,type,created_at,size,title FROM tag_relations WHERE type=?1 AND key IN (SELECT value FROM json_each(?2)) ORDER BY rowid")?.query_map(params![kind,json(keys)?],|r|Ok(RelationRecord{tag_id:r.get(0)?,key:r.get(1)?,kind:r.get(2)?,created_at:r.get(3)?,size_bytes:r.get(4)?,title:r.get(5)?}))?.collect::<rusqlite::Result<Vec<_>>>())?)
}
fn json(items: &[String]) -> rusqlite::Result<String> {
    serde_json::to_string(items).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}
fn validate(c: &Connection, id: &str, kind: i32) -> LibraryResult<()> {
    if tag::io::tag_by_id(c, id)?.is_none_or(|t| t.kind != kind) {
        return Err(LibraryError::Other("tag type mismatch".into()));
    }
    Ok(())
}
fn insert(c: &Connection, items: &[RelationInput]) -> LibraryResult<()> {
    for item in items {
        if item.key.is_empty() || item.size_bytes < 0 {
            return Err(LibraryError::Other("invalid tag relation".into()));
        }
        validate(c, &item.tag_id, item.kind)?;
        c.execute("INSERT INTO tag_relations (tag_id,key,type,created_at,size,title) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(tag_id,key,type) DO UPDATE SET size=excluded.size,title=excluded.title",params![item.tag_id,item.key,item.kind,crate::utils::dbtime::now_iso_millis(),item.size_bytes,item.title])?;
    }
    Ok(())
}
fn transaction<T>(db: &Db, f: impl FnOnce(&Connection) -> LibraryResult<T>) -> LibraryResult<T> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let result = f(&tx)?;
        tag::io::touch_counts(&tx)?;
        tx.commit()?;
        Ok(result)
    })
}
pub fn add(db: &Db, items: &[RelationInput]) -> LibraryResult<()> {
    transaction(db, |c| insert(c, items))
}
pub fn edit(
    db: &Db,
    kind: i32,
    key: &str,
    title: &str,
    size_bytes: i64,
    add_ids: &[String],
    remove_ids: &[String],
) -> LibraryResult<()> {
    if key.is_empty() || size_bytes < 0 {
        return Err(LibraryError::Other("invalid tag relation".into()));
    }
    transaction(db, |c| {
        for id in add_ids.iter().chain(remove_ids) {
            validate(c, id, kind)?;
        }
        insert(
            c,
            &add_ids
                .iter()
                .map(|id| RelationInput {
                    tag_id: id.clone(),
                    key: key.into(),
                    kind,
                    title: title.into(),
                    size_bytes,
                })
                .collect::<Vec<_>>(),
        )?;
        tag::io::remove_relations(c, &[key.into()], remove_ids)?;
        Ok(())
    })
}
pub fn remove(db: &Db, keys: &[String], ids: &[String]) -> LibraryResult<()> {
    transaction(db, |c| {
        let mut kind = None;
        for id in ids {
            if let Some(t) = tag::io::tag_by_id(c, id)? {
                if kind.is_some_and(|k| k != t.kind) {
                    return Err(LibraryError::Other("tag type mismatch".into()));
                }
                kind = Some(t.kind);
            }
        }
        tag::io::remove_relations(c, keys, ids)?;
        Ok(())
    })
}
pub fn remove_keys(db: &Db, kind: i32, keys: &[String]) -> LibraryResult<()> {
    transaction(db, |c| {
        c.execute(
            "DELETE FROM tag_relations WHERE type=?1 AND key IN (SELECT value FROM json_each(?2))",
            params![kind, json(keys)?],
        )?;
        Ok(())
    })
}
pub fn clear_type(db: &Db, kind: i32) -> LibraryResult<()> {
    transaction(db, |c| {
        c.execute("DELETE FROM tag_relations WHERE type=?", [kind])?;
        Ok(())
    })
}
pub fn clear_tag(db: &Db, id: &str) -> LibraryResult<()> {
    transaction(db, |c| {
        c.execute("DELETE FROM tag_relations WHERE tag_id=?", [id])?;
        Ok(())
    })
}
pub fn intersection(db: &Db, ids: &[String]) -> LibraryResult<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    db.with_conn(|c| {
        let mut kind=None;
        for id in ids {
            let Some(t)=tag::io::tag_by_id(c,id)? else {return Ok(Vec::new());};
            if kind.is_some_and(|k|k!=t.kind) {return Err(LibraryError::Other("tag type mismatch".into()));}
            kind=Some(t.kind);
        }
        Ok(c.prepare("SELECT key FROM tag_relations WHERE tag_id IN (SELECT value FROM json_each(?1)) GROUP BY key HAVING COUNT(DISTINCT tag_id)=(SELECT COUNT(DISTINCT value) FROM json_each(?1)) ORDER BY MIN(rowid)")?.query_map([json(ids)?],|r|r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}
#[cfg(test)]
#[path = "../../tests/unit/library/tag_records.rs"]
mod tests;
