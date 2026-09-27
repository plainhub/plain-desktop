//! Row IO for `tags` + `tag_relations` (plain-app Room `Tag` /
//! `TagRelation` shapes). Counts are computed by subquery on read —
//! there is no stored count to drift.

use rusqlite::params;

use super::LibraryDb;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    /// Numeric plain-app `DataType` ordinal (0=DEFAULT, 1=AUDIO, 2=VIDEO,
    /// 3=IMAGE, …).
    pub kind: i32,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRelationRow {
    pub tag_id: String,
    pub key: String,
}

const TAG_COLUMNS: &str =
    "id,name,type,(SELECT COUNT(*) FROM tag_relations r WHERE r.tag_id = tags.id)";

/// All tags of one kind in insertion order.
pub fn tags_by_type(db: &LibraryDb, kind: i32) -> Vec<TagRow> {
    db.with_conn(|conn| {
        let mut stmt = match conn.prepare(&format!(
            "SELECT {TAG_COLUMNS} FROM tags WHERE type=? ORDER BY rowid ASC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![kind], row_to_tag)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

pub fn tag_by_id(db: &LibraryDb, id: &str) -> Option<TagRow> {
    db.with_conn(|conn| {
        conn.query_row(
            &format!("SELECT {TAG_COLUMNS} FROM tags WHERE id=?"),
            params![id],
            row_to_tag,
        )
        .ok()
    })
}

fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRow> {
    Ok(TagRow {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        count: row.get::<_, i64>(3)? as i32,
    })
}

pub fn insert_tag(db: &LibraryDb, tag: &TagRow) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO tags (id,name,type) VALUES (?1,?2,?3)",
            params![tag.id, tag.name, tag.kind],
        );
    })
}

pub fn update_tag_name(db: &LibraryDb, id: &str, name: &str) -> bool {
    db.with_conn(|conn| {
        conn.execute("UPDATE tags SET name=?1 WHERE id=?2", params![name, id])
            .map(|n| n > 0)
            .unwrap_or(false)
    })
}

/// Delete a tag and all of its relations.
pub fn delete_tag(db: &LibraryDb, id: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute("DELETE FROM tag_relations WHERE tag_id=?", params![id]);
        let _ = conn.execute("DELETE FROM tags WHERE id=?", params![id]);
    })
}

/// Relations of one key (media id), tag-id order.
pub fn relations_for_key(db: &LibraryDb, key: &str) -> Vec<TagRelationRow> {
    db.with_conn(|conn| {
        let mut stmt = match conn
            .prepare("SELECT tag_id,key FROM tag_relations WHERE key=? ORDER BY tag_id ASC")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![key], row_to_relation)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

/// Relations for several keys filtered to one tag kind — the
/// `tagRelations(type, keys)` query shape, one statement per key so the
/// result keeps the input key order.
pub fn relations_for_keys_of_kind(
    db: &LibraryDb,
    keys: &[String],
    kind: i32,
) -> Vec<TagRelationRow> {
    db.with_conn(|conn| {
        let mut out = Vec::new();
        for key in keys {
            let Ok(mut stmt) = conn.prepare(
                "SELECT r.tag_id, r.key FROM tag_relations r \
                 JOIN tags t ON t.id = r.tag_id WHERE r.key=? AND t.type=?",
            ) else {
                continue;
            };
            if let Ok(rows) = stmt.query_map(params![key, kind], row_to_relation) {
                out.extend(rows.filter_map(|r| r.ok()));
            }
        }
        out
    })
}

fn row_to_relation(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRelationRow> {
    Ok(TagRelationRow {
        tag_id: row.get(0)?,
        key: row.get(1)?,
    })
}

/// Keys (media ids) currently related to `tag_id`.
pub fn keys_for_tag(db: &LibraryDb, tag_id: &str) -> Vec<String> {
    db.with_conn(|conn| {
        let mut stmt =
            match conn.prepare("SELECT key FROM tag_relations WHERE tag_id=? ORDER BY rowid ASC") {
                Ok(s) => s,
                Err(_) => return vec![],
            };
        stmt.query_map(params![tag_id], |row| row.get::<_, String>(0))
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

/// Insert relations, skipping empty ids and exact duplicates
/// (`INSERT OR IGNORE` — same no-duplicate semantics as plain-app's
/// `addToTags`).
pub fn insert_relations(db: &LibraryDb, rels: &[(String, String)]) {
    db.with_conn(|conn| {
        for (tag_id, key) in rels {
            if tag_id.is_empty() || key.is_empty() {
                continue;
            }
            let _ = conn.execute(
                "INSERT OR IGNORE INTO tag_relations (tag_id,key) VALUES (?1,?2)",
                params![tag_id, key],
            );
        }
    })
}

/// Remove the (tag_id × key) cross product.
pub fn remove_relations(db: &LibraryDb, keys: &[String], tag_ids: &[String]) {
    if keys.is_empty() || tag_ids.is_empty() {
        return;
    }
    db.with_conn(|conn| {
        let key_ph = (1..=keys.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let tag_ph = (keys.len() + 1..=keys.len() + tag_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql =
            format!("DELETE FROM tag_relations WHERE key IN ({key_ph}) AND tag_id IN ({tag_ph})");
        let mut all: Vec<&String> = keys.iter().collect();
        all.extend(tag_ids.iter());
        let _ = conn.execute(&sql, rusqlite::params_from_iter(all));
    })
}

/// Remove every relation of the given keys (media deleted / trashed
/// cascade).
pub fn remove_relations_for_keys(db: &LibraryDb, keys: &[String]) {
    if keys.is_empty() {
        return;
    }
    db.with_conn(|conn| {
        let placeholders = (1..=keys.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("DELETE FROM tag_relations WHERE key IN ({placeholders})");
        let _ = conn.execute(&sql, rusqlite::params_from_iter(keys.iter()));
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/tag.rs"]
mod tests;
