use crate::graphql::resolvers::bookmarks::bookmark;
use crate::graphql::resolvers::favorites::favorite_folder;
use crate::graphql::resolvers::feeds::feed;
use crate::graphql::resolvers::files::app_file;
use crate::graphql::resolvers::images::image_editor_project;
use crate::graphql::resolvers::notes::note;
use crate::graphql::resolvers::pomodoro::pomodoro;
use async_graphql::{EmptySubscription, MergedObject, Schema};
#[derive(MergedObject, Default)]
pub struct Query(
    #[cfg(feature = "media")] crate::graphql::resolvers::media::scan::MediaScanQuery,
    crate::graphql::resolvers::discovery::host::DiscoveryHostQuery,
    crate::graphql::resolvers::chat::public::ChatQuery,
    crate::graphql::resolvers::chat::host::ChatHostQuery,
    crate::graphql::resolvers::settings::public::PrefsQuery,
    crate::graphql::resolvers::settings::state::PreferencesQuery,
    crate::graphql::resolvers::files::tasks::FileTaskQuery,
    crate::graphql::resolvers::audio::host::AudioHostQuery,
    crate::graphql::resolvers::audio::playback::AudioPlaybackQuery,
    crate::graphql::resolvers::videos::progress::VideoProgressQuery,
    crate::graphql::resolvers::shares::host::ShareHostQuery,
    app_file::AppFileQuery,
    favorite_folder::FavoriteFolderQuery,
    crate::graphql::resolvers::files::app_files_host::AppFileHostQuery,
    note::NoteQuery,
    feed::FeedQuery,
    crate::graphql::resolvers::tags::host::TagQuery,
    crate::graphql::resolvers::tags::access::TagHostQuery,
    crate::graphql::resolvers::media::aux::MediaAuxQuery,
    crate::graphql::resolvers::images::host_index::ImageIndexQuery,
    crate::graphql::resolvers::clipboard::public::ClipboardQuery,
    bookmark::BookmarkQuery,
    pomodoro::PomodoroQuery,
    crate::graphql::resolvers::pomodoro::host::PomodoroRecordQuery,
    image_editor_project::ImageEditorProjectQuery,
    crate::graphql::resolvers::images::editor_host::ImageEditorItemsQuery,
);
#[derive(MergedObject, Default)]
pub struct Mutation(
    #[cfg(feature = "media")] crate::graphql::resolvers::media::scan::MediaScanMutation,
    crate::graphql::resolvers::discovery::host::DiscoveryHostMutation,
    crate::graphql::resolvers::chat::public::ChatMutation,
    crate::graphql::resolvers::chat::host::ChatHostMutation,
    crate::graphql::resolvers::settings::state::PreferencesMutation,
    crate::graphql::resolvers::files::tasks::FileTaskMutation,
    crate::graphql::resolvers::audio::host::AudioHostMutation,
    crate::graphql::resolvers::audio::playback::AudioPlaybackMutation,
    crate::graphql::resolvers::videos::progress::VideoProgressMutation,
    crate::graphql::resolvers::shares::host::ShareHostMutation,
    crate::graphql::resolvers::files::app_files_host::AppFileHostMutation,
    favorite_folder::FavoriteFolderMutation,
    note::NoteMutation,
    feed::FeedMutation,
    crate::graphql::resolvers::tags::host::TagMutation,
    crate::graphql::resolvers::tags::access::TagHostMutation,
    crate::graphql::resolvers::media::aux::MediaAuxMutation,
    crate::graphql::resolvers::media::actions::MediaActionMutation,
    crate::graphql::resolvers::images::host_index::ImageIndexMutation,
    crate::graphql::resolvers::clipboard::public::ClipboardMutation,
    crate::graphql::resolvers::clipboard::host::ClipboardHostMutation,
    bookmark::BookmarkMutation,
    crate::graphql::resolvers::bookmarks::metadata::BookmarkMetadataMutation,
    pomodoro::PomodoroMutation,
    crate::graphql::resolvers::pomodoro::host::PomodoroHostMutation,
    image_editor_project::ImageEditorProjectMutation,
);
pub type ContentSchema = Schema<Query, Mutation, EmptySubscription>;
pub fn build(
    db: std::sync::Arc<plain_rs::db::Db>,
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    prefs: std::sync::Arc<plain_rs::prefs::Prefs>,
    directory: std::path::PathBuf,
) -> ContentSchema {
    build_with_host(
        db,
        events,
        prefs,
        directory,
        std::sync::Arc::new(crate::host::bridge::Host::default()),
    )
}
pub(crate) fn build_with_host(
    db: std::sync::Arc<plain_rs::db::Db>,
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    prefs: std::sync::Arc<plain_rs::prefs::Prefs>,
    directory: std::path::PathBuf,
    host: std::sync::Arc<crate::host::bridge::Host>,
) -> ContentSchema {
    let services = crate::assembly::services::Services::new(
        db.clone(),
        host.clone(),
        events.clone(),
        prefs.clone(),
        directory.clone(),
    );
    build_with_services(db, events, prefs, host, services)
}
pub(crate) fn build_with_services(
    db: std::sync::Arc<plain_rs::db::Db>,
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    prefs: std::sync::Arc<plain_rs::prefs::Prefs>,
    host: std::sync::Arc<crate::host::bridge::Host>,
    services: crate::assembly::services::Services,
) -> ContentSchema {
    services
        .configure_schema(
            Schema::build(Query::default(), Mutation::default(), EmptySubscription),
            db,
            host,
            prefs,
            events,
        )
        .finish()
}

#[cfg(test)]
#[path = "../../../tests/unit/graphql/schemas/host.rs"]
mod tests;
