//! Filesystem watching for the media catalog.
//!
//! `notify` reports what changed; this decides what that means for the
//! catalog. The mapping is deliberately small and pure so it can be tested
//! without a real watcher: a file that appeared or changed is read, a file
//! that went away is forgotten, and anything the scan policy excludes is
//! forgotten rather than indexed.

use std::path::Path;

use notify::event::{EventKind, ModifyKind, RemoveKind};
use notify::{Event, RecursiveMode, Watcher};
use plain_rs::enums::DataType;

/// What a filesystem event means for the catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Effect {
    /// Read the file and record whatever it is.
    Index,
    /// Drop the path and everything keyed to it.
    Forget,
    /// Re-read a whole subtree, because a directory boundary moved.
    RescanSubtree,
    /// Nothing: a hidden file, or a kind we do not model.
    Ignore,
}

fn media_type_of(path: &Path) -> Option<DataType> {
    let name = path.file_name()?.to_str()?;
    let kind = plain_media::scan::infer_type(name);
    [
        DataType::Audio,
        DataType::Video,
        DataType::Image,
        DataType::Doc,
    ]
    .into_iter()
    .find(|data_type| data_type.media_type_str() == Some(kind))
}

/// Decide what one path in an event means.
///
/// `is_excluded` is the scan policy's own answer for that path, so the
/// watcher and a manual scan agree on what counts as media.
pub(crate) fn effect_for(path: &Path, kind: &EventKind, is_excluded: bool) -> Effect {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Effect::Ignore;
    };
    // A marker file changes what a directory contains, not what one file is.
    if name == ".nomedia" {
        return Effect::RescanSubtree;
    }
    // Editors and sync clients write dotfiles the user never asked to index.
    if name.starts_with('.') {
        return Effect::Ignore;
    }
    match kind {
        EventKind::Create(_) => {
            if path.is_file() && media_type_of(path).is_some() {
                if is_excluded {
                    Effect::Forget
                } else {
                    Effect::Index
                }
            } else {
                Effect::Ignore
            }
        }
        EventKind::Remove(RemoveKind::Folder) => Effect::RescanSubtree,
        EventKind::Remove(_) => Effect::Forget,
        // A rename arrives as delete-then-create; the create arm indexes it.
        EventKind::Modify(ModifyKind::Name(_)) => Effect::Ignore,
        EventKind::Modify(_) => {
            if path.is_file() && media_type_of(path).is_some() {
                if is_excluded {
                    Effect::Forget
                } else {
                    Effect::Index
                }
            } else {
                Effect::Ignore
            }
        }
        _ => Effect::Ignore,
    }
}

/// The live watcher. Dropping it stops the watch.
pub(crate) struct WatcherHandle {
    _watcher: Box<dyn Watcher + Send>,
}

pub(crate) fn start<F>(roots: &[std::path::PathBuf], on_event: F) -> Result<WatcherHandle, String>
where
    F: Fn(&Path, &EventKind) + Send + Sync + 'static,
{
    let mut watcher =
        notify::recommended_watcher(move |result: notify::Result<Event>| match result {
            Ok(event) => {
                for path in &event.paths {
                    on_event(path, &event.kind);
                }
            }
            Err(error) => log::warn!("media watch: {error}"),
        })
        .map_err(|error| error.to_string())?;
    for root in roots {
        if root.exists() {
            if let Err(error) = watcher.watch(root, RecursiveMode::Recursive) {
                log::warn!("media watch {}: {error}", root.display());
            }
        }
    }
    Ok(WatcherHandle {
        _watcher: Box::new(watcher),
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_watch.rs"]
mod tests;
