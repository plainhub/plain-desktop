//! The public `/graphql` schema — the contract LAN clients (the web console)
//! query.
//!
//! Deliberately separate from [`super::schema::ContentSchema`]: that one is
//! the app's own host-facing API and carries internal roots
//! (`audioHostQueueItems`, `fileHostTasks`, …) that must never reach the
//! network. Here only the contract's root fields exist, so anything missing
//! from [`crate::content_types`] is a gap to fill rather than a leak.

use super::public_audio::{AudioMutation, AudioQuery};
use super::public_calls::{CallsMutation, CallsQuery};
use super::public_clipboard::{ClipboardMutation, ClipboardQuery};
use super::public_chat::{ChatMutation, ChatQuery};
use super::public_contacts::{ContactsMutation, ContactsQuery};
use super::public_device::{DeviceMutation, DeviceQuery};
use super::public_feeds::{FeedsMutation, FeedsQuery};
use super::public_file_ops::{FileOpsMutation, MediaActionMutation, UploadQuery};
use super::public_files::{FilesQuery, FavoritesMutation};
use super::public_image_index::{ImageIndexMutation, ImageIndexQuery};
use super::public_db::{DbMutation, DbQuery};
use super::public_media::MediaQuery;
use super::public_peers::{PeersMutation, PeersQuery};
use super::public_prefs::{PrefsMutation, PrefsQuery};
use super::public_tags::{TagsMutation, TagsQuery};
use super::public_notifications::{NotificationsMutation, NotificationsQuery};
use super::public_notes::{NotesMutation, NotesQuery};
use super::public_packages::{PackagesMutation, PackagesQuery};
use super::public_screen_mirror::{ScreenMirrorMutation, ScreenMirrorQuery, SettingsMutation};
use super::public_sms::{SmsMutation, SmsQuery};
use super::schema;
use crate::content_api::host::Host;
use crate::{db::Db, prefs::Prefs};
use async_graphql::{EmptySubscription, MergedObject, Schema};
use std::sync::Arc;

// Roots that already serve contract fields for the app's own API are
// mounted here unchanged rather than reimplemented: for these the contract
// and the host schema were byte-identical, so a second copy would only be a
// place for the two to drift.
//
// Deliberately *not* among them:
//   - `tags`, `favoriteFolders` and the clipboard trio — both sides define
//     them, and the host copies return the app's wider types (an ungated
//     clipboard), so the contract wins.
//   - the note and feed roots — `Note.tags` is the app's `Tag`, which
//     carries a numeric kind the contract does not have, so mounting them
//     would put two `Tag` types in one registry. Those get public roots.
//
// This is a plain comment, not a doc comment on purpose: async-graphql
// turns `///` on the root object into the published `Query` description,
// and a note about which roots are reused is not part of the contract.
#[derive(MergedObject, Default)]
pub struct Query(
    schema::app_file::AppFileQuery,
    schema::bookmark::BookmarkQuery,
    schema::pomodoro::PomodoroQuery,
    schema::image_editor_project::ImageEditorProjectQuery,
    NotesQuery,
    FeedsQuery,
    AudioQuery,
    ChatQuery,
    PeersQuery,
    PackagesQuery,
    NotificationsQuery,
    ClipboardQuery,
    ContactsQuery,
    SmsQuery,
    CallsQuery,
    ScreenMirrorQuery,
    ImageIndexQuery,
    FilesQuery,
    MediaQuery,
    TagsQuery,
    PrefsQuery,
    DbQuery,
    UploadQuery,
    DeviceQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(
    schema::bookmark::BookmarkMutation,
    schema::pomodoro::PomodoroMutation,
    schema::image_editor_project::ImageEditorProjectMutation,
    NotesMutation,
    FeedsMutation,
    AudioMutation,
    ChatMutation,
    PeersMutation,
    PackagesMutation,
    NotificationsMutation,
    ClipboardMutation,
    ContactsMutation,
    SmsMutation,
    CallsMutation,
    ScreenMirrorMutation,
    ImageIndexMutation,
    SettingsMutation,
    FavoritesMutation,
    TagsMutation,
    PrefsMutation,
    DbMutation,
    FileOpsMutation,
    MediaActionMutation,
    DeviceMutation,
);

pub type PublicSchema = Schema<Query, Mutation, EmptySubscription>;

pub fn build(
    host: Arc<Host>,
    events: tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
) -> PublicSchema {
    Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(host.clone())
        .data(Arc::new(super::audio::Audio::new(db.clone(), host)))
        .data(events.clone())
        .data(crate::app_files::FileStore::new(
            db.clone(),
            directory.clone(),
        ))
        // Both constructors already hand back an `Arc`, and the resolvers ask
        // for `Arc<Service>` / `Arc<SyncService>` — wrapping again here
        // registered `Arc<Arc<_>>` and every pomodoro and feed-sync call
        // failed with "Data ... does not exist".
        .data(crate::pomodoro::Service::new(
            db.clone(),
            prefs.clone(),
            events.clone(),
        ))
        .data(crate::image_editor::Updates::new(events.clone()))
        .data(crate::feeds::SyncService::new(
            db.clone(),
            events,
            Some(Arc::new(crate::feeds::FeedAssets {
                db: db.clone(),
                directory: directory.clone(),
            })),
        ))
        .data(prefs)
        .data(db.clone())
        .data(Arc::new(crate::feeds::FeedAssets { db, directory }))
        .finish()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_schema.rs"]
mod tests;
