use rusqlite::Connection;

#[cfg(feature = "chat")]
mod app_file;
#[cfg(feature = "library")]
mod audio;
#[cfg(feature = "chat")]
mod bookmark;
#[cfg(feature = "chat")]
mod chat;
#[cfg(feature = "library")]
mod favorite_folder;
#[cfg(feature = "library")]
mod feeds;
#[cfg(feature = "library")]
mod image_editor_project;
#[cfg(feature = "chat")]
mod nearby_device;
#[cfg(feature = "library")]
mod notes;
#[cfg(feature = "chat")]
mod peer;
#[cfg(feature = "library")]
mod tag;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    #[cfg(feature = "chat")]
    chat::init(conn)?;
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
    tag::init(conn)?;
    #[cfg(feature = "library")]
    favorite_folder::init(conn)?;
    #[cfg(feature = "library")]
    image_editor_project::init(conn)?;
    #[cfg(feature = "library")]
    notes::init(conn)?;
    #[cfg(feature = "library")]
    feeds::init(conn)?;
    Ok(())
}
