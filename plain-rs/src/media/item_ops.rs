use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};

use crate::db::Db as SqlDb;
use crate::library::audio_queue;
use crate::media::image_index::{self, MediaSort};
use crate::media::kv::Db;
use crate::media::{scan, trash};

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
    let uuids = if !ids.trim().is_empty() {
        split_ids(&ids)
    } else {
        let trash_filter = match action {
            MediaItemsAction::Restore | MediaItemsAction::Delete => Some(true),
            MediaItemsAction::Trash => None,
        };
        match image_index::global().search(
            &text,
            media_type,
            trash_filter,
            MediaSort::DateDesc,
            0,
            10_000,
        ) {
            Ok(rows) => rows.into_iter().map(|row| row.uuid).collect(),
            Err(error) => {
                log::error!("[media-items] search failed: {error}");
                Vec::new()
            }
        }
    };

    for uuid in &uuids {
        let media = match scan::get_by_uuid(db, uuid) {
            Ok(Some(media)) => media,
            _ => continue,
        };
        if let Err(error) = apply_media_items_action(db, library, &media, action).await {
            log::debug!("[media-items] {action:?} {}: {error}", media.path);
        }
    }
    Ok(i32::try_from(uuids.len()).unwrap_or(i32::MAX))
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
    let (ids, text) = bulk_selection(query)?;
    let uuids = if !ids.trim().is_empty() {
        split_ids(&ids)
    } else {
        match image_index::global().search(&text, media_type, None, MediaSort::DateDesc, 0, 10_000)
        {
            Ok(rows) => rows.into_iter().map(|row| row.uuid).collect(),
            Err(error) => {
                log::error!("[media-items] move search failed: {error}");
                Vec::new()
            }
        }
    };

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
            let _ = audio_queue::remove_paths(library, std::slice::from_ref(&media.path));
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
    ids.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
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
) -> Result<()> {
    match action {
        MediaItemsAction::Trash => {
            if media.is_trash {
                return Ok(());
            }
            if media.r#type == "audio" {
                audio_queue::remove_paths(library, std::slice::from_ref(&media.path));
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
            for key in [media.uuid.clone(), media.path.clone()] {
                crate::library::tags::remove_relations_for_keys(library, &[key]);
            }
            scan::upsert_media_row(db, &updated)?;
        }
        MediaItemsAction::Restore => {
            if !media.is_trash {
                return Ok(());
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
            if media.r#type == "audio" {
                audio_queue::remove_paths(library, std::slice::from_ref(&media.path));
            }
            if trash::is_trashed_path(&media.path) {
                trash::delete_trash_by_path(&media.path).await?;
            } else {
                let path = std::path::Path::new(&media.path);
                if path.is_file() {
                    std::fs::remove_file(path)?;
                } else if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                }
            }
            scan::delete_by_uuid(db, &media.uuid)?;
        }
    }
    Ok(())
}
