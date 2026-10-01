//! Media stack shared by plain-nas and the plain-desktop local API:
//! fjall KV storage, file/media indexes, the media scanner, thumbnail
//! engine, metadata extraction, file-management helpers, and the
//! watcher. Ported from plain-nas so every desktop platform
//! (macOS / Windows / Linux) can browse local files through the same
//! GraphQL surface the NAS serves.

pub mod catalog;
pub mod config;
pub mod cover;
pub mod eventbus;
pub mod file_browse;
pub mod file_ops;
pub mod file_tasks;
pub mod fsx;
pub mod image_index;
pub mod index;
pub mod item_ops;
pub mod kv;
#[cfg(feature = "library")]
pub mod library_tracks;
pub mod lyrics;
pub mod metadata;
pub mod mountinfo;
pub mod paths;
pub mod pdf_preview;
pub mod scan;
pub mod search;
pub mod service;
pub mod tagging;
pub mod thumb;
pub mod trash;
pub mod uuid;
pub mod video;
pub mod walk;
pub mod watcher;
