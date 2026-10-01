//! Shared SQLite persistence exposed by the plain-rs domains.

pub use crate::db::bookmark::{
    DBookmark, DBookmarkGroup, delete_bookmark_group, delete_bookmarks, get_bookmark_by_id,
    get_bookmark_group_by_id, get_bookmark_groups, get_bookmarks, get_bookmarks_by_group_id,
    insert_bookmark, insert_bookmark_group, update_bookmark, update_bookmark_group,
};
pub use crate::db::{
    Db, DAppFile, DChannel, DChat, DNearbyDeviceCache, DPeer, iso_from_unix_millis, now_iso,
    now_millis,
};

pub use crate::library::tags as tag_store;
