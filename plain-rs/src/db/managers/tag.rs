//! Row IO for `tags` + `tag_relations` (plain-app Room `Tag` /
//! `TagRelation` shapes).

use rusqlite::params;

use crate::db::Db;
pub use crate::db::models::tag::{TagRelationRow, TagRow};

const TAG_COLUMNS: &str = "id,name,type,count";

/// All tags of one kind in insertion order.
pub fn tags_by_type(db: &Db, kind: i32) -> Vec<TagRow> {
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

pub fn tag_by_id(db: &Db, id: &str) -> Option<TagRow> {
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

pub fn insert_tag(db: &Db, tag: &TagRow) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO tags (id,name,type,count,created_at,updated_at) VALUES (?1,?2,?3,0,?4,?4)",
            params![tag.id, tag.name, tag.kind, crate::utils::dbtime::now_iso()],
        );
    })
}

pub fn update_tag_name(db: &Db, id: &str, name: &str) -> bool {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE tags SET name=?1,updated_at=?2 WHERE id=?3",
            params![name, crate::utils::dbtime::now_iso(), id],
        )
        .map(|n| n > 0)
        .unwrap_or(false)
    })
}

/// Delete a tag and all of its relations.
pub fn delete_tag(db: &Db, id: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute("DELETE FROM tag_relations WHERE tag_id=?", params![id]);
        let _ = conn.execute("DELETE FROM tags WHERE id=?", params![id]);
    })
}

/// Relations of one key (media id), tag-id order.
pub fn relations_for_key(db: &Db, key: &str) -> Vec<TagRelationRow> {
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
pub fn relations_for_keys_of_kind(db: &Db, keys: &[String], kind: i32) -> Vec<TagRelationRow> {
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
pub fn keys_for_tag(db: &Db, tag_id: &str) -> Vec<String> {
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
pub fn insert_relations(db: &Db, rels: &[(String, String)]) {
    db.with_conn(|conn| {
        for (tag_id, key) in rels {
            if tag_id.is_empty() || key.is_empty() {
                continue;
            }
            let _ = conn.execute(
                "INSERT OR IGNORE INTO tag_relations (tag_id,key,type,created_at,size,title)                  SELECT id,?2,type,?3,0,'' FROM tags WHERE id=?1",
                params![tag_id, key, crate::utils::dbtime::now_iso()],
            );
            let _ = conn.execute(
                "UPDATE tags SET count=(SELECT COUNT(*) FROM tag_relations WHERE tag_id=tags.id),updated_at=?2 WHERE id=?1",
                params![tag_id, crate::utils::dbtime::now_iso()],
            );
        }
    })
}

/// Remove the (tag_id × key) cross product.
pub fn remove_relations(db: &Db, keys: &[String], tag_ids: &[String]) {
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
        let _ = conn.execute(
            "UPDATE tags SET count=(SELECT COUNT(*) FROM tag_relations WHERE tag_id=tags.id)",
            [],
        );
    })
}

/// Remove every relation of the given keys (media deleted / trashed
/// cascade).
pub fn remove_relations_for_keys(db: &Db, keys: &[String]) {
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
        let _ = conn.execute(
            "UPDATE tags SET count=(SELECT COUNT(*) FROM tag_relations WHERE tag_id=tags.id)",
            [],
        );
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/tag.rs"]
mod tests;
