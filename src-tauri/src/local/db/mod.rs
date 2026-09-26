//! Local persistence exposed by the shared plain-rs cores: `chat`
//! (messages/channels/peers/bookmarks) and `library` (audio
//! queue/playlists/history, tags, favorite folders).

pub use plain_rs::chat::db::bookmark::{
    DBookmark, DBookmarkGroup, delete_bookmark_group, delete_bookmarks, get_bookmark_by_id,
    get_bookmark_group_by_id, get_bookmark_groups, get_bookmarks, get_bookmarks_by_group_id,
    insert_bookmark, insert_bookmark_group, update_bookmark, update_bookmark_group,
};
pub use plain_rs::chat::db::{
    ChatDb, DAppFile, DChannel, DChat, DNearbyDeviceCache, DPeer, iso_from_unix_millis, now_iso,
    now_millis,
};

pub use plain_rs::library::db::LibraryDb;
pub use plain_rs::library::tags as tag_store;
