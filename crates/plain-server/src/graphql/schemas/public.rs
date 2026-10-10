//! The public `/graphql` schema — the contract LAN clients (the web console)
//! query.
//!
//! Deliberately separate from [`crate::graphql::schemas::host::ContentSchema`]: that one is
//! the app's own host-facing API and carries internal roots
//! (`audioHostQueueItems`, `fileHostTasks`, …) that must never reach the
//! network. Here only the contract's root fields exist, so anything missing
//! from [`crate::graphql::types`] is a gap to fill rather than a leak.
#[cfg(feature = "mobile")]
use crate::mobile::graphql::MobileMutation as MobileMutationImpl;
#[cfg(feature = "mobile")]
use crate::mobile::graphql::MobileQuery as MobileQueryImpl;
#[cfg(not(feature = "mobile"))]
type MobileQueryImpl = async_graphql::MergedObjectTail;
#[cfg(not(feature = "mobile"))]
type MobileMutationImpl = async_graphql::MergedObjectTail;
use crate::graphql::resolvers::docs::public::DocsQuery;
use crate::graphql::resolvers::images::public::ImagesQuery;
use crate::graphql::resolvers::videos::public::VideosQuery;

use crate::graphql::resolvers::audio::public::AudioMutation;
use crate::graphql::resolvers::audio::public::AudioQuery;
use crate::graphql::resolvers::chat::public::ChatMutation;
use crate::graphql::resolvers::chat::public::ChatQuery;
use crate::graphql::resolvers::clipboard::public::ClipboardMutation;
use crate::graphql::resolvers::clipboard::public::ClipboardQuery;
use crate::graphql::resolvers::database::public::DbMutation;
use crate::graphql::resolvers::database::public::DbQuery;
use crate::graphql::resolvers::device::public::DeviceMutation;
use crate::graphql::resolvers::device::public::DeviceQuery;
use crate::graphql::resolvers::feeds::public::FeedsMutation;
use crate::graphql::resolvers::feeds::public::FeedsQuery;
use crate::graphql::resolvers::files::operations::FileOpsMutation;
use crate::graphql::resolvers::files::operations::MediaActionMutation;
use crate::graphql::resolvers::files::operations::UploadQuery;
use crate::graphql::resolvers::files::public::FavoritesMutation;
use crate::graphql::resolvers::files::public::FilesQuery;
use crate::graphql::resolvers::images::public_index::ImageIndexMutation;
use crate::graphql::resolvers::images::public_index::ImageIndexQuery;
use crate::graphql::resolvers::media::public::MediaQuery;
use crate::graphql::resolvers::notes::public::NotesMutation;
use crate::graphql::resolvers::notes::public::NotesQuery;
use crate::graphql::resolvers::notifications::public::NotificationsMutation;
use crate::graphql::resolvers::notifications::public::NotificationsQuery;
use crate::graphql::resolvers::peers::public::PeersMutation;
use crate::graphql::resolvers::peers::public::PeersQuery;
use crate::graphql::resolvers::screen_mirror::public::ScreenMirrorMutation;
use crate::graphql::resolvers::screen_mirror::public::ScreenMirrorQuery;
use crate::graphql::resolvers::screen_mirror::public::SettingsMutation;
use crate::graphql::resolvers::settings::public::PrefsMutation;
use crate::graphql::resolvers::settings::public::PrefsQuery;
use crate::graphql::resolvers::tags::public::TagsMutation;
use crate::graphql::resolvers::tags::public::TagsQuery;
use crate::host::bridge::Host;
use async_graphql::{EmptySubscription, MergedObject, Schema};
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
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
    MobileQueryImpl,
    crate::graphql::resolvers::files::app_file::AppFileQuery,
    crate::graphql::resolvers::bookmarks::bookmark::BookmarkQuery,
    crate::graphql::resolvers::pomodoro::pomodoro::PomodoroQuery,
    crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectQuery,
    NotesQuery,
    FeedsQuery,
    AudioQuery,
    ChatQuery,
    PeersQuery,
    NotificationsQuery,
    ClipboardQuery,
    ScreenMirrorQuery,
    ImageIndexQuery,
    FilesQuery,
    MediaQuery,
    ImagesQuery,
    VideosQuery,
    DocsQuery,
    TagsQuery,
    PrefsQuery,
    DbQuery,
    UploadQuery,
    DeviceQuery,
);

#[derive(MergedObject, Default)]
pub struct Mutation(
    MobileMutationImpl,
    crate::graphql::resolvers::bookmarks::bookmark::BookmarkMutation,
    crate::graphql::resolvers::pomodoro::pomodoro::PomodoroMutation,
    crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectMutation,
    NotesMutation,
    FeedsMutation,
    AudioMutation,
    ChatMutation,
    PeersMutation,
    NotificationsMutation,
    ClipboardMutation,
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
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
) -> PublicSchema {
    let services = crate::assembly::services::Services::new(
        db.clone(),
        host.clone(),
        events.clone(),
        prefs.clone(),
    );
    let models = crate::services::images::models::Runtime::new(
        directory.clone(),
        host.clone(),
        prefs.clone(),
        services.index.clone(),
        events.clone(),
    );
    #[cfg(feature = "mobile")]
    let mobile = crate::mobile::Runtime::new(
        host.clone(),
        prefs.clone(),
        db.clone(),
        directory.clone(),
        events.clone(),
    );
    build_with_runtime(
        host,
        events,
        prefs,
        db,
        directory,
        services,
        models,
        #[cfg(feature = "mobile")]
        mobile,
    )
}
pub(crate) fn build_with_runtime(
    host: Arc<Host>,
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
    services: crate::assembly::services::Services,
    models: Arc<crate::services::images::models::Runtime>,
    #[cfg(feature = "mobile")] mobile: Arc<crate::mobile::Runtime>,
) -> PublicSchema {
    let builder = Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(Arc::new(
            plain_rs::files::uploads::Runtime::with_completion({
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
            }),
        ))
        .data(host.clone())
        .data(services.audio)
        .data(services.index)
        .data(models)
        .data(events.clone())
        .data(plain_rs::files::app_files::FileStore::new(
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
        .data(plain_rs::media::images::editor::Updates::new(
            events.clone(),
        ))
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
        }));
    #[cfg(feature = "mobile")]
    let builder = crate::mobile::graphql::configure(builder, mobile);
    builder.finish()
}

#[cfg(test)]
#[path = "../../../tests/unit/graphql/schemas/public.rs"]
mod tests;
