use rusqlite::{OptionalExtension, params};

use crate::db::Db;
pub use crate::db::models::image_editor_project::ImageEditorProjectRow;

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImageEditorProjectRow> {
    Ok(ImageEditorProjectRow {
        id: row.get(0)?,
        state_b64: row.get(1)?,
        thumbnail: row.get(2)?,
        canvas_width: row.get(3)?,
        canvas_height: row.get(4)?,
        layer_count: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const COLUMNS: &str =
    "id,state_b64,thumbnail,canvas_width,canvas_height,layer_count,created_at,updated_at";

pub fn get(db: &Db, id: &str) -> rusqlite::Result<Option<ImageEditorProjectRow>> {
    db.with_conn(|conn| {
        conn.query_row(
            &format!("SELECT {COLUMNS} FROM image_editor_projects WHERE id=?1"),
            params![id],
            from_row,
        )
        .optional()
    })
}

pub fn list(db: &Db, limit: i32) -> rusqlite::Result<Vec<ImageEditorProjectRow>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM image_editor_projects ORDER BY updated_at DESC,id LIMIT ?1"
        ))?;
        stmt.query_map(params![limit.clamp(0, 100)], from_row)?
            .collect()
    })
}

pub fn summaries(
    db: &Db,
    offset: i32,
    limit: i32,
    query: &str,
) -> rusqlite::Result<Vec<ImageEditorProjectRow>> {
    let text = crate::utils::search_dsl::field_value(query, "text").unwrap_or_default();
    let pattern = format!(
        "%{}%",
        text.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id,'',thumbnail,canvas_width,canvas_height,layer_count,created_at,updated_at FROM image_editor_projects WHERE id LIKE ?1 ESCAPE '\\' ORDER BY updated_at DESC,id LIMIT ?2 OFFSET ?3")?;
        stmt.query_map(params![pattern,limit.clamp(0,100),offset.max(0)],from_row)?.collect()
    })
}

pub fn save(
    db: &Db,
    id: &str,
    state_b64: &str,
    thumbnail: Option<&str>,
    canvas_width: i32,
    canvas_height: i32,
    layer_count: i32,
) -> rusqlite::Result<ImageEditorProjectRow> {
    let now = crate::utils::dbtime::now_iso_millis();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO image_editor_projects
                (id,state_b64,thumbnail,canvas_width,canvas_height,layer_count,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?7)
             ON CONFLICT(id) DO UPDATE SET state_b64=excluded.state_b64,
                thumbnail=excluded.thumbnail,canvas_width=excluded.canvas_width,
                canvas_height=excluded.canvas_height,layer_count=excluded.layer_count,
                updated_at=excluded.updated_at",
            params![id, state_b64, thumbnail, canvas_width, canvas_height, layer_count, now],
        )?;
        conn.query_row(
            &format!("SELECT {COLUMNS} FROM image_editor_projects WHERE id=?1"),
            params![id],
            from_row,
        )
    })
}

pub fn delete(db: &Db, id: &str) -> rusqlite::Result<bool> {
    db.with_conn(|conn| {
        Ok(conn.execute("DELETE FROM image_editor_projects WHERE id=?1", params![id])? > 0)
    })
}
