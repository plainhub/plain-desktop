use crate::{
    db::Db,
    library::{LibraryError, LibraryResult},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::params;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingInput {
    pub id: String,
    pub path: String,
    pub embedding_base64: String,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub image_id: String,
    pub score: f32,
}
fn vector(bytes: &[u8]) -> LibraryResult<Vec<f32>> {
    if bytes.is_empty() || bytes.len() % 4 != 0 || bytes.len() > 4096 * 4 {
        return Err(LibraryError::Other("invalid embedding dimensions".into()));
    }
    let values = bytes
        .chunks_exact(4)
        .map(|c| f32::from_be_bytes(c.try_into().unwrap()))
        .collect::<Vec<_>>();
    if values.iter().any(|v| !v.is_finite()) {
        return Err(LibraryError::Other("invalid embedding values".into()));
    }
    Ok(values)
}
fn decode(value: &str) -> LibraryResult<Vec<u8>> {
    STANDARD
        .decode(value)
        .map_err(|e| LibraryError::Other(e.to_string()))
}
pub fn save(db: &Db, items: &[EmbeddingInput]) -> LibraryResult<()> {
    if items.len() > 100 {
        return Err(LibraryError::Other("embedding batch too large".into()));
    }
    let blobs = items
        .iter()
        .map(|i| {
            if i.id.is_empty() || i.path.is_empty() {
                return Err(LibraryError::Other("missing embedding identity".into()));
            }
            let blob = decode(&i.embedding_base64)?;
            vector(&blob)?;
            Ok(blob)
        })
        .collect::<LibraryResult<Vec<_>>>()?;
    db.with_conn(|c| {
        let tx=c.unchecked_transaction()?;
        for (i,blob) in items.iter().zip(blobs) {
            tx.execute("INSERT INTO image_embeddings (id,path,embedding,created_at,updated_at) VALUES (?1,?2,?3,?4,?4) ON CONFLICT(id) DO UPDATE SET path=excluded.path,embedding=excluded.embedding,updated_at=excluded.updated_at",params![i.id,i.path,blob,crate::utils::dbtime::now_iso_millis()])?;
        }
        tx.commit()?;Ok(())
    })
}
pub fn search(db: &Db, query: &str, limit: usize) -> LibraryResult<Vec<SearchResult>> {
    if limit > 500 {
        return Err(LibraryError::Other("image search limit too large".into()));
    }
    let query = vector(&decode(query)?)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    db.with_conn(|c| {
        let mut statement = c.prepare("SELECT id,embedding FROM image_embeddings ORDER BY id")?;
        let mut rows = statement.query([])?;
        let mut top = Vec::<SearchResult>::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let values = vector(&row.get::<_, Vec<u8>>(1)?)?;
            if values.len() != query.len() {
                return Err(LibraryError::Other("embedding dimension mismatch".into()));
            }
            let score = query
                .iter()
                .zip(&values)
                .fold(0.0_f32, |sum, (a, b)| sum + a * b);
            if !score.is_finite() {
                return Err(LibraryError::Other("invalid image search score".into()));
            }
            if score < 0.15 {
                continue;
            }
            top.push(SearchResult {
                image_id: id,
                score,
            });
            top.sort_by(|a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.image_id.cmp(&b.image_id))
            });
            top.truncate(limit);
        }
        Ok(top)
    })
}
pub fn ids(db: &Db) -> LibraryResult<Vec<String>> {
    Ok(db.embedding_ids()?)
}
pub fn count(db: &Db) -> LibraryResult<i64> {
    Ok(db.embedding_count()?)
}
pub fn delete(db: &Db, ids: &[String]) -> LibraryResult<usize> {
    Ok(db.embedding_delete_by_ids(ids)?)
}
pub fn clear(db: &Db) -> LibraryResult<usize> {
    Ok(db.embedding_delete_all()?)
}
#[cfg(test)]
#[path = "../../tests/unit/library/image_embeddings.rs"]
mod tests;
