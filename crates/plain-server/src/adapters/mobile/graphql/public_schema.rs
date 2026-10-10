//! The public `/graphql` schema — the contract LAN clients (the web console)
//! query.
//!
//! Deliberately separate from [`super::schema::ContentSchema`]: that one is
//! the app's own host-facing API and carries internal roots
//! (`audioHostQueueItems`, `fileHostTasks`, …) that must never reach the
//! network. Here only the contract's root fields exist, so anything missing
//! from [`crate::graphql::types`] is a gap to fill rather than a leak.

use crate::adapters::mobile::files::public_file_ops::{
    FileOpsMutation, MediaActionMutation, UploadQuery,
};
use crate::adapters::mobile::files::public_files::{FavoritesMutation, FilesQuery};
use crate::adapters::mobile::graphql::public_chat::{ChatMutation, ChatQuery};
use crate::adapters::mobile::graphql::public_clipboard::{ClipboardMutation, ClipboardQuery};
use crate::adapters::mobile::graphql::public_db::{DbMutation, DbQuery};
use crate::adapters::mobile::graphql::public_feeds::{FeedsMutation, FeedsQuery};
use crate::adapters::mobile::graphql::public_notes::{NotesMutation, NotesQuery};
use crate::adapters::mobile::graphql::public_peers::{PeersMutation, PeersQuery};
use crate::adapters::mobile::graphql::public_prefs::{PrefsMutation, PrefsQuery};
use crate::adapters::mobile::media::public_audio::{AudioMutation, AudioQuery};
use crate::adapters::mobile::media::public_image_index::{ImageIndexMutation, ImageIndexQuery};
use crate::adapters::mobile::media::public_media::MediaQuery;
use crate::adapters::mobile::media::public_tags::{TagsMutation, TagsQuery};
use crate::adapters::mobile::providers::public_calls::{CallsMutation, CallsQuery};
use crate::adapters::mobile::providers::public_contacts::{ContactsMutation, ContactsQuery};
use crate::adapters::mobile::providers::public_device::{DeviceMutation, DeviceQuery};
use crate::adapters::mobile::providers::public_notifications::{
    NotificationsMutation, NotificationsQuery,
};
use crate::adapters::mobile::providers::public_packages::{PackagesMutation, PackagesQuery};
use crate::adapters::mobile::providers::public_screen_mirror::{
    ScreenMirrorMutation, ScreenMirrorQuery, SettingsMutation,
};
use crate::adapters::mobile::providers::public_sms::{SmsMutation, SmsQuery};
use crate::adapters::mobile::runtime::host::Host;
use async_graphql::{EmptySubscription, MergedObject, Schema};
use plain_rs::{db::Db, prefs::Prefs};
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
    crate::graphql::shared::app_file::AppFileQuery,
    crate::graphql::shared::bookmark::BookmarkQuery,
    crate::graphql::shared::pomodoro::PomodoroQuery,
    crate::graphql::shared::image_editor_project::ImageEditorProjectQuery,
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
    crate::graphql::shared::bookmark::BookmarkMutation,
    crate::graphql::shared::pomodoro::PomodoroMutation,
    crate::graphql::shared::image_editor_project::ImageEditorProjectMutation,
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
    events: tokio::sync::broadcast::Sender<plain_rs::ws_event::WsEvent>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
) -> PublicSchema {
    let mms = crate::adapters::mobile::providers::mms_send::Runtime::new(
        host.clone(),
        prefs.clone(),
        db.clone(),
        directory.clone(),
        events.clone(),
    );
    build_with_mms(host, events, prefs, db, directory, mms)
}
pub(in crate::adapters::mobile) fn build_with_mms(
    host: Arc<Host>,
    events: tokio::sync::broadcast::Sender<plain_rs::ws_event::WsEvent>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
    mms: Arc<crate::adapters::mobile::providers::mms_send::Runtime>,
) -> PublicSchema {
    let services = crate::adapters::mobile::runtime::services::Services::new(
        db.clone(),
        host.clone(),
        events.clone(),
        prefs.clone(),
    );
    let models = crate::adapters::mobile::media::image_models::Runtime::new(
        directory.clone(),
        host.clone(),
        prefs.clone(),
        services.index.clone(),
        events.clone(),
    );
    build_with_runtime(host, events, prefs, db, directory, mms, services, models)
}
pub(crate) fn build_with_runtime(
    host: Arc<Host>,
    events: tokio::sync::broadcast::Sender<plain_rs::ws_event::WsEvent>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
    mms: Arc<crate::adapters::mobile::providers::mms_send::Runtime>,
    services: crate::adapters::mobile::runtime::services::Services,
    models: Arc<crate::adapters::mobile::media::image_models::Runtime>,
) -> PublicSchema {
    Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(mms)
        .data(Arc::new(
            crate::adapters::mobile::providers::phone_geo::Runtime::default(),
        ))
        .data(Arc::new(plain_rs::uploads::Runtime::with_completion({
            let host = host.clone();
            move |path| {
                let host = host.clone();
                Box::pin(async move {
                    if let Err(error) = host
                        .call(
                            "scanFilesFacts",
                            serde_json::json!({"paths": [path.to_string_lossy()]}),
                        )
                        .await
                    {
                        log::warn!("Merged upload media scan failed: {error}");
                    }
                })
            }
        })))
        .data(host.clone())
        .data(services.audio)
        .data(services.index)
        .data(models)
        .data(events.clone())
        .data(plain_rs::app_files::FileStore::new(
            db.clone(),
            directory.clone(),
        ))
        // Both constructors already hand back an `Arc`, and the resolvers ask
        // for `Arc<Service>` / `Arc<SyncService>` — wrapping again here
        // registered `Arc<Arc<_>>` and every pomodoro and feed-sync call
        // failed with "Data ... does not exist".
        .data(crate::services::pomodoro::Service::new(
            db.clone(),
            prefs.clone(),
            events.clone(),
        ))
        .data(plain_rs::image_editor::Updates::new(events.clone()))
        .data(crate::services::feeds::SyncService::new(
            db.clone(),
            events,
            Some(Arc::new(crate::services::feeds::FeedAssets {
                db: db.clone(),
                directory: directory.clone(),
            })),
        ))
        .data(prefs)
        .data(db.clone())
        .data(Arc::new(crate::services::feeds::FeedAssets {
            db,
            directory,
        }))
        .finish()
}

#[cfg(test)]
#[path = "../../../../tests/unit/adapters/mobile/graphql/public_schema.rs"]
mod tests;
