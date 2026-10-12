//! Projects one shared media row into the `systemMediaRows` facts objects the
//! GraphQL resolvers already decode, adding no field the protocol lacks.
//! Trash membership stays out of the projection: the catalog query decides
//! which rows exist, so a selected trashed row projects like any other.

use plain_rs::db::MediaFileRow;
use plain_rs::enums::DataType;
use serde_json::{Value, json};

fn media_type(kind: i32) -> Result<DataType, String> {
    [
        DataType::Audio,
        DataType::Video,
        DataType::Image,
        DataType::Doc,
    ]
    .into_iter()
    .find(|data_type| data_type.kind() == kind)
    .ok_or_else(|| format!("Unsupported media facts type {kind}"))
}

pub fn row_facts(row: &MediaFileRow) -> Result<Value, String> {
    let mut facts = json!({
        "id": row.id,
        "title": row.title,
        "path": row.path,
        "size": row.size_bytes,
        "bucketId": row.bucket_id,
        "createdAt": row.created_at,
        "updatedAt": row.updated_at,
        "isFavorite": row.is_favorite,
    });
    match media_type(row.data_type)? {
        DataType::Audio => {
            facts["artist"] = json!(row.artist);
            facts["durationMs"] = json!(row.duration_ms);
            facts["albumFileId"] = json!(row.album_file_id);
        }
        _ => {
            facts["durationMs"] = json!(row.duration_ms);
            facts["takenAt"] = json!(row.taken_at);
            facts["width"] = json!(row.width);
            facts["height"] = json!(row.height);
            facts["rotation"] = json!(row.rotation);
        }
    }
    Ok(facts)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_catalog_facts.rs"]
mod tests;
