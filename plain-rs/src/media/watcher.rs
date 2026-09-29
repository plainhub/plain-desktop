//! File system watcher using `notify` (inotify on Linux).
//!
//! Watches configured root directories for file create/write/remove/rename
//! events and updates the media index + search index accordingly.
//!
//! Port of Go's `cmd/services/watcher/run.go` + `pkg/watcher/watcher.go`.

use anyhow::Result;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Start watching the given root directories for file changes.
/// Returns a `WatcherHandle` that keeps the watcher alive.
pub fn start_watching(db: Arc<crate::media::kv::Db>, roots: &[PathBuf]) -> Result<WatcherHandle> {
    let db_clone = db.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| match res {
            Ok(event) => handle_event(&db_clone, &event),
            Err(e) => log::error!("[watcher] error: {e}"),
        },
        Config::default(),
    )?;

    for root in roots {
        if root.exists() {
            if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
                log::error!("[watcher] failed to watch {}: {e}", root.display());
            } else {
                log::info!("[watcher] watching {}", root.display());
            }
        }
    }

    Ok(WatcherHandle { _watcher: watcher })
}

/// Handle that keeps the file watcher alive. Drop it to stop watching.
pub struct WatcherHandle {
    _watcher: RecommendedWatcher,
}

/// Build missing indexes at startup. Mirrors Go's `cmd/services/watcher/run.go`.
/// `default_roots` is the host's policy for which directories the file
/// search index should cover when it needs building from scratch
/// (plain-nas: its /mnt/usb* mounts; desktop: the media source dirs).
pub fn build_missing_indexes(
    data_dir: &Path,
    db: &crate::media::kv::Db,
    default_roots: &[PathBuf],
) {
    if !crate::media::index::FileSearchIndex::exists(data_dir) {
        let roots: Vec<PathBuf> = default_roots.to_vec();
        if !roots.is_empty() {
            log::info!("[watcher] building file search index");
            match crate::media::index::FileSearchIndex::open(data_dir) {
                Ok(idx) => {
                    if let Err(e) = idx.build_index(&roots) {
                        log::error!("[watcher] file index build failed: {e}");
                    }
                }
                Err(e) => log::error!("[watcher] file index open failed: {e}"),
            }
        }
    }

    heal_media_index(db);
}

/// Rebuild the process-wide media search index from the KV rows when it
/// serves no documents. The index directory can exist yet be empty (e.g.
/// freshly recreated after a schema change); without this the GraphQL media
/// counts stay 0 until someone triggers a manual rebuild.
pub fn heal_media_index(db: &crate::media::kv::Db) -> usize {
    let idx = crate::media::image_index::global();
    heal_media_index_at(&idx, db)
}

fn heal_media_index_at(
    idx: &crate::media::image_index::MediaSearchIndex,
    db: &crate::media::kv::Db,
) -> usize {
    if idx.doc_count() > 0 {
        log::info!(
            "[watcher] media index ok: {} docs — no heal needed",
            idx.doc_count()
        );
        return 0;
    }
    // An empty library (fresh install, nothing scanned yet) has nothing to
    // rebuild from — cheap point-check instead of counting all rows. This
    // MUST stay loud: a prefix scan that silently returns empty on a
    // non-empty library leaves the index unserved with zero trace.
    let kv_has_rows = db.scan_prefix(b"media:uuid:").next().is_some();
    if !kv_has_rows {
        log::warn!(
            "[watcher] media index empty AND no media:uuid: KV rows found — nothing to heal"
        );
        return 0;
    }
    log::info!("[watcher] media index empty with KV rows present — rebuilding from KV");
    match idx.build_from_db(db) {
        Ok(n) => {
            log::info!("[watcher] media index rebuilt from KV: {n} docs");
            n
        }
        Err(e) => {
            log::error!("[watcher] media index rebuild failed: {e}");
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

fn handle_event(db: &Arc<crate::media::kv::Db>, event: &notify::Event) {
    for path in &event.paths {
        let path_str = path.to_string_lossy().to_string();

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == ".nomedia" {
            if let Some(parent) = path.parent() {
                schedule_subtree_rescan(db, parent);
            }
            continue;
        }
        if name.starts_with('.') {
            continue;
        }

        match event.kind {
            EventKind::Create(_) => {
                if path.is_file() {
                    sync_file(db, &path_str);
                }
            }
            EventKind::Remove(notify::event::RemoveKind::Folder) => {
                if let Err(error) = crate::media::scan::delete_by_path(db, &path_str) {
                    log::error!("[watcher] delete_by_path({path_str}): {error}");
                }
                schedule_subtree_rescan(db, path);
            }
            EventKind::Remove(_) => {
                let _ = crate::media::scan::delete_by_path(db, &path_str);
            }
            EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                // Rename/move: remove old path, scan new.
                let _ = crate::media::scan::delete_by_path(db, &path_str);
                if path.exists() && path.is_file() {
                    sync_file(db, &path_str);
                } else {
                    schedule_subtree_rescan(db, path);
                }
            }
            EventKind::Modify(notify::event::ModifyKind::Metadata(_)) if path.is_dir() => {
                schedule_subtree_rescan(db, path);
            }
            EventKind::Modify(_) => {
                if path.is_file() {
                    sync_file(db, &path_str);
                }
            }
            _ => {}
        }
    }
}

fn schedule_subtree_rescan(db: &Arc<crate::media::kv::Db>, path: &Path) {
    let db = db.clone();
    let root = path.to_path_buf();
    std::thread::spawn(move || {
        if let Err(error) = crate::media::scan::rescan_subtree(db, root.clone()) {
            log::error!(
                "[watcher] subtree update failed for {}: {error}",
                root.display()
            );
        }
    });
}

fn sync_file(db: &crate::media::kv::Db, path: &str) {
    if crate::media::scan::is_media_excluded(path) {
        if let Err(error) = crate::media::scan::delete_by_path(db, path) {
            log::error!("[watcher] excluded file cleanup failed for {path}: {error}");
        }
    } else if let Err(error) = crate::media::scan::scan_file(db, path) {
        log::debug!("[watcher] scan_file({path}): {error}");
    }
}

#[cfg(test)]
#[path = "../../tests/unit/media/watcher.rs"]
mod tests;
