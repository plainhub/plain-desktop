use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};

use crate::db::Db as SqlDb;
use crate::enums::DataType;
use crate::library::{audio_queue, media_actions};
use crate::media::image_index::{self, MediaSort};
use crate::media::kv::Db;
use crate::media::{scan, trash};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MediaItemsAction {
    Trash,
    Restore,
    Delete,
}

pub async fn run_media_items_action(
    db: &Arc<Db>,
    library: &SqlDb,
    media_type: Option<&str>,
    query: &str,
    action: MediaItemsAction,
) -> Result<i32> {
    let trash_filter = match action {
        MediaItemsAction::Restore | MediaItemsAction::Delete => Some(true),
        MediaItemsAction::Trash => None,
    };
    let uuids = select_ids(query, media_type, trash_filter)?;
    let mut affected = 0_i32;
    let mut failures = Vec::new();
    for uuid in uuids {
        let result = async {
            let media = scan::get_by_uuid(db, &uuid)?
                .ok_or_else(|| anyhow::anyhow!("media item not found"))?;
            require_media_type(&media, media_type)?;
            apply_media_items_action(db, library, &media, action).await
        }
        .await;
        match result {
            Ok(true) => {
                affected = affected
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("media action count overflow"))?
            }
            Ok(false) => {}
            Err(error) => failures.push(format!("{uuid}: {error}")),
        }
    }
    if !failures.is_empty() {
        bail!(
            "{action:?}: {} media actions failed ({affected} completed): {}",
            failures.len(),
            failures.join("; ")
        );
    }
    Ok(affected)
}

fn require_media_type(media: &scan::MediaFile, expected: Option<&str>) -> Result<()> {
    if expected.is_some_and(|kind| !kind.is_empty() && kind != media.r#type) {
        bail!("media type mismatch");
    }
    Ok(())
}

fn select_ids(
    query: &str,
    media_type: Option<&str>,
    trash_filter: Option<bool>,
) -> Result<Vec<String>> {
    let (ids, _) = bulk_selection(query)?;
    if crate::utils::search_dsl::parse(query)
        .iter()
        .any(|field| field.name == "ids")
    {
        return Ok(split_ids(&ids));
    }
    let index = image_index::global();
    search_ids(&index, query, media_type, trash_filter)
}

fn search_ids(
    index: &image_index::MediaSearchIndex,
    query: &str,
    media_type: Option<&str>,
    trash_filter: Option<bool>,
) -> Result<Vec<String>> {
    let expected = index.count(query, media_type, trash_filter)?;
    let mut result = Vec::with_capacity(expected);
    let mut seen = HashSet::new();
    loop {
        let page = index.search(
            query,
            media_type,
            trash_filter,
            MediaSort::DateDesc,
            result.len(),
            512,
        )?;
        if page.is_empty() {
            break;
        }
        for row in page {
            if !seen.insert(row.uuid.clone()) {
                bail!("media selection changed; retry");
            }
            result.push(row.uuid);
        }
    }
    if result.len() != expected {
        bail!("media selection changed; retry");
    }
    Ok(result)
}

pub async fn move_media_items(
    db: &Arc<Db>,
    library: &SqlDb,
    media_type: Option<&str>,
    query: &str,
    dest_dir: &str,
) -> Result<i32> {
    let dest = PathBuf::from(dest_dir);
    if !dest.is_dir() {
        bail!("dest_dir is not a directory: {dest_dir}");
    }
    let uuids = select_ids(query, media_type, None)?;

    let index = image_index::global();
    let mut moved = 0_i32;
    for uuid in &uuids {
        let media = match scan::get_by_uuid(db, uuid) {
            Ok(Some(media)) => media,
            _ => continue,
        };
        let new_path = dest.join(file_name_of(&media.path));
        let renamed = std::fs::rename(&media.path, &new_path).is_ok()
            || (std::fs::copy(&media.path, &new_path).is_ok()
                && std::fs::remove_file(&media.path).is_ok());
        if !renamed {
            log::debug!(
                "[media-items] move {} -> {}: failed",
                media.path,
                new_path.display()
            );
            continue;
        }
        let mut updated = media.clone();
        updated.path = new_path.to_string_lossy().into_owned();
        if scan::upsert_media_row(db, &updated).is_err() {
            continue;
        }
        let _ = index.remove_by_uuid(uuid);
        let _ = index.add_media_file(&updated);
        if updated.r#type == "audio" {
            audio_queue::remove_paths(library, std::slice::from_ref(&media.path))?;
        }
        moved += 1;
    }
    let _ = index.commit();
    Ok(moved)
}

fn bulk_selection(query: &str) -> Result<(String, String)> {
    let fields = crate::utils::search_dsl::parse(query);
    if fields.is_empty() {
        bail!("bulk_query_required");
    }
    let ids = fields
        .iter()
        .filter(|field| field.name == "ids")
        .map(|field| field.value.clone())
        .last()
        .unwrap_or_default();
    let text = fields
        .iter()
        .filter(|field| field.name == "text")
        .map(|field| field.value.clone())
        .last()
        .unwrap_or_default();
    Ok((ids, text))
}

fn split_ids(ids: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn file_name_of(path: &str) -> String {
    std::path::Path::new(path.trim_end_matches('/'))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string()
}

async fn apply_media_items_action(
    db: &Arc<Db>,
    library: &SqlDb,
    media: &scan::MediaFile,
    action: MediaItemsAction,
) -> Result<bool> {
    match action {
        MediaItemsAction::Trash => {
            if media.is_trash {
                return Ok(false);
            }
            let trashed = trash::trash_paths(vec![media.path.clone()]).await?;
            let mut updated = media.clone();
            updated.path = trashed
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("trash failed"))?;
            updated.trash_path = updated.path.clone();
            if updated.original_path.is_empty() {
                updated.original_path = media.path.clone();
            }
            updated.is_trash = true;
            updated.deleted_at = chrono::Utc::now().timestamp();
            cleanup_references(library, media, media_actions::Action::Trash)?;
            scan::upsert_media_row(db, &updated)?;
        }
        MediaItemsAction::Restore => {
            if !media.is_trash {
                return Ok(false);
            }
            let restored = trash::restore_paths(vec![media.path.clone()]).await?;
            let mut updated = media.clone();
            updated.path = restored
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("restore failed"))?;
            updated.is_trash = false;
            updated.trash_path.clear();
            updated.deleted_at = 0;
            scan::upsert_media_row(db, &updated)?;
        }
        MediaItemsAction::Delete => {
            if trash::is_trashed_path(&media.path) {
                trash::delete_trash_by_path(&media.path).await?;
            } else {
                crate::media::fsx::remove(std::path::Path::new(&media.path)).await?;
            }
            cleanup_references(library, media, media_actions::Action::Delete)?;
            scan::delete_by_uuid(db, &media.uuid)?;
            image_index::global().remove_by_uuid(&media.uuid)?;
        }
    }
    Ok(true)
}

fn cleanup_references(
    library: &SqlDb,
    media: &scan::MediaFile,
    action: media_actions::Action,
) -> Result<()> {
    let kind = match media.r#type.as_str() {
        "audio" => DataType::Audio,
        "video" => DataType::Video,
        "image" => DataType::Image,
        "doc" => DataType::Doc,
        _ => DataType::File,
    };
    let mut keys = vec![media.uuid.clone(), media.path.clone()];
    if !media.original_path.is_empty() {
        keys.push(media.original_path.clone());
    }
    let items = keys
        .into_iter()
        .map(|id| media_actions::Item {
            path: if id == media.uuid {
                media.path.clone()
            } else {
                id.clone()
            },
            id,
            destination_path: String::new(),
        })
        .collect::<Vec<_>>();
    media_actions::cleanup(library, kind, action, &items)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/media/item_ops.rs"]
mod tests;
