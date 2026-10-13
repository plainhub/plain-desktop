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
use crate::graphql::resolvers::favorites::favorite_folder::{
    FavoriteFolderMutation, FavoriteFolderQuery,
};
use crate::graphql::resolvers::feeds::feed::FeedMutation;
use crate::graphql::resolvers::feeds::feed::FeedQuery;
use crate::graphql::resolvers::files::operations::FileOpsMutation;
use crate::graphql::resolvers::files::operations::MediaActionMutation;
use crate::graphql::resolvers::files::operations::UploadQuery;
use crate::graphql::resolvers::files::public::FilesQuery;
use crate::graphql::resolvers::images::public_index::ImageIndexMutation;
use crate::graphql::resolvers::images::public_index::ImageIndexQuery;
use crate::graphql::resolvers::media::public::MediaQuery;
use crate::graphql::resolvers::notes::note::NoteMutation;
use crate::graphql::resolvers::notes::note::NoteQuery;
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

#[derive(MergedObject, Default)]
pub struct Query(
    #[cfg(feature = "media")] crate::graphql::resolvers::media::scan::MediaScanPublicQuery,
    crate::graphql::resolvers::app::platform::PlatformQuery,
    crate::graphql::resolvers::settings::state::PreferencesQuery,
    MobileQueryImpl,
    crate::graphql::resolvers::files::app_file::AppFileQuery,
    crate::graphql::resolvers::bookmarks::bookmark::BookmarkQuery,
    crate::graphql::resolvers::pomodoro::pomodoro::PomodoroQuery,
    crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectQuery,
    NoteQuery,
    FeedQuery,
    AudioQuery,
    ChatQuery,
    PeersQuery,
    NotificationsQuery,
    ClipboardQuery,
    ScreenMirrorQuery,
    ImageIndexQuery,
    FilesQuery,
    FavoriteFolderQuery,
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
    crate::graphql::resolvers::app::platform::PlatformMutation,
    #[cfg(feature = "media")] crate::graphql::resolvers::media::scan::MediaScanPublicMutation,
    crate::graphql::resolvers::settings::state::PreferencesMutation,
    MobileMutationImpl,
    crate::graphql::resolvers::bookmarks::bookmark::BookmarkMutation,
    crate::graphql::resolvers::pomodoro::pomodoro::PomodoroMutation,
    crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectMutation,
    NoteMutation,
    FeedMutation,
    AudioMutation,
    ChatMutation,
    PeersMutation,
    NotificationsMutation,
    ClipboardMutation,
    ScreenMirrorMutation,
    ImageIndexMutation,
    SettingsMutation,
    FavoriteFolderMutation,
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
        directory.clone(),
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
    services: crate::assembly::services::Services,
    models: Arc<crate::services::images::models::Runtime>,
    #[cfg(feature = "mobile")] mobile: Arc<crate::mobile::Runtime>,
) -> PublicSchema {
    let builder = Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .register_output_type::<crate::graphql::resolvers::media::item::MediaItem>()
        .data(models);
    let builder = services.configure_schema(builder, db, host, prefs, events);
    #[cfg(feature = "mobile")]
    let builder = crate::mobile::graphql::configure(builder, mobile);
    builder.finish()
}

#[cfg(test)]
#[path = "../../../tests/unit/graphql/schemas/public.rs"]
mod tests;
