use rusqlite::Connection;

#[cfg(feature = "chat")]
mod app_file;
#[cfg(feature = "library")]
mod archived_conversation;
#[cfg(feature = "library")]
mod audio;
#[cfg(feature = "chat")]
mod bookmark;
#[cfg(feature = "chat")]
mod chat;
#[cfg(feature = "library")]
mod clipboard;
#[cfg(feature = "library")]
mod favorite_folder;
#[cfg(feature = "library")]
mod feeds;
#[cfg(feature = "library")]
mod file_task;
#[cfg(feature = "library")]
mod image_editor_project;
#[cfg(feature = "library")]
mod image_embedding;
#[cfg(feature = "library")]
mod media_item;
#[cfg(feature = "chat")]
mod nearby_device;
#[cfg(feature = "library")]
mod notes;
#[cfg(feature = "chat")]
mod peer;
#[cfg(feature = "library")]
mod pomodoro;
#[cfg(feature = "library")]
mod session;
#[cfg(feature = "library")]
mod share;
#[cfg(feature = "library")]
mod tag;
#[cfg(feature = "library")]
mod trashed_sms;
#[cfg(feature = "library")]
mod video_play_progress;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    #[cfg(feature = "library")]
    archived_conversation::init(conn)?;
    #[cfg(feature = "chat")]
    chat::init(conn)?;
    #[cfg(feature = "library")]
    clipboard::init(conn)?;
    #[cfg(feature = "chat")]
    peer::init(conn)?;
    #[cfg(feature = "chat")]
    nearby_device::init(conn)?;
    #[cfg(feature = "chat")]
    app_file::init(conn)?;
    #[cfg(feature = "chat")]
    bookmark::init(conn)?;
    #[cfg(feature = "library")]
    audio::init(conn)?;
    #[cfg(feature = "library")]
    image_embedding::init(conn)?;
    #[cfg(feature = "library")]
    tag::init(conn)?;
    #[cfg(feature = "library")]
    media_item::init(conn)?;
    #[cfg(feature = "library")]
    pomodoro::init(conn)?;
    #[cfg(feature = "library")]
    session::init(conn)?;
    #[cfg(feature = "library")]
    share::init(conn)?;
    #[cfg(feature = "library")]
    favorite_folder::init(conn)?;
    #[cfg(feature = "library")]
    file_task::init(conn)?;
    #[cfg(feature = "library")]
    image_editor_project::init(conn)?;
    #[cfg(feature = "library")]
    notes::init(conn)?;
    #[cfg(feature = "library")]
    feeds::init(conn)?;
    #[cfg(feature = "library")]
    trashed_sms::init(conn)?;
    #[cfg(feature = "library")]
    video_play_progress::init(conn)?;
    Ok(())
}
