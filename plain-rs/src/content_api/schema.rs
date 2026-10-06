pub use crate::content_types as types;
#[path = "../http_server/main_schemas/app_file.rs"]
pub(crate) mod app_file;
mod app_files_host;
mod audio_host;
mod audio_playback;
#[path = "../http_server/main_schemas/bookmark.rs"]
pub(crate) mod bookmark;
#[path = "../http_server/main_schemas/bookmark_types.rs"]
mod bookmark_types;
mod bookmarks;
pub(crate) mod clipboard;
#[path = "../http_server/main_schemas/content_common.rs"]
pub(crate) mod content_common;
#[path = "../http_server/main_schemas/favorite_folder.rs"]
mod favorite_folder;
#[path = "../http_server/main_schemas/feed.rs"]
pub(crate) mod feed;
mod file_tasks;
mod image_editor_host;
#[path = "../http_server/main_schemas/image_editor_project.rs"]
pub(crate) mod image_editor_project;
mod image_index;
mod media_actions;
mod media_aux;
#[path = "../http_server/main_schemas/note.rs"]
mod note;
#[path = "../http_server/main_schemas/pomodoro.rs"]
pub(crate) mod pomodoro;
mod pomodoro_host;
mod shares_host;
mod tags;
mod tags_host;
mod video_progress;
use async_graphql::{EmptySubscription, MergedObject, Schema};
#[derive(MergedObject, Default)]
pub struct Query(
    file_tasks::FileTaskQuery,
    audio_host::AudioHostQuery,
    audio_playback::AudioPlaybackQuery,
    video_progress::VideoProgressQuery,
    shares_host::ShareHostQuery,
    app_file::AppFileQuery,
    favorite_folder::FavoriteFolderQuery,
    app_files_host::AppFileHostQuery,
    note::NoteQuery,
    feed::FeedQuery,
    tags::TagQuery,
    tags_host::TagHostQuery,
    media_aux::MediaAuxQuery,
    image_index::ImageIndexQuery,
    clipboard::ClipboardQuery,
    bookmark::BookmarkQuery,
    pomodoro::PomodoroQuery,
    pomodoro_host::PomodoroRecordQuery,
    image_editor_project::ImageEditorProjectQuery,
    image_editor_host::ImageEditorItemsQuery,
);
#[derive(MergedObject, Default)]
pub struct Mutation(
    file_tasks::FileTaskMutation,
    audio_host::AudioHostMutation,
    audio_playback::AudioPlaybackMutation,
    video_progress::VideoProgressMutation,
    shares_host::ShareHostMutation,
    app_files_host::AppFileHostMutation,
    favorite_folder::FavoriteFolderMutation,
    note::NoteMutation,
    feed::FeedMutation,
    tags::TagMutation,
    tags_host::TagHostMutation,
    media_aux::MediaAuxMutation,
    media_actions::MediaActionMutation,
    image_index::ImageIndexMutation,
    clipboard::ClipboardMutation,
    bookmark::BookmarkMutation,
    bookmarks::BookmarkMetadataMutation,
    pomodoro::PomodoroMutation,
    pomodoro_host::PomodoroHostMutation,
    image_editor_project::ImageEditorProjectMutation,
);
pub type ContentSchema = Schema<Query, Mutation, EmptySubscription>;
pub fn build(
    db: std::sync::Arc<crate::db::Db>,
    events: tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>,
    prefs: std::sync::Arc<crate::prefs::Prefs>,
    directory: std::path::PathBuf,
) -> ContentSchema {
    build_with_host(
        db,
        events,
        prefs,
        directory,
        std::sync::Arc::new(super::host::Host::default()),
    )
}
pub(crate) fn build_with_host(
    db: std::sync::Arc<crate::db::Db>,
    events: tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>,
    prefs: std::sync::Arc<crate::prefs::Prefs>,
    directory: std::path::PathBuf,
    host: std::sync::Arc<super::host::Host>,
) -> ContentSchema {
    let services =
        super::services::Services::new(db.clone(), host.clone(), events.clone(), prefs.clone());
    build_with_services(db, events, prefs, directory, host, services)
}
pub(crate) fn build_with_services(
    db: std::sync::Arc<crate::db::Db>,
    events: tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>,
    prefs: std::sync::Arc<crate::prefs::Prefs>,
    directory: std::path::PathBuf,
    host: std::sync::Arc<super::host::Host>,
    services: super::services::Services,
) -> ContentSchema {
    Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(services.files)
        .data(services.audio)
        .data(services.index)
        .data(host)
        .data(db.clone())
        .data(crate::shares::Service::new(db.clone(), prefs.clone()))
        .data(crate::app_files::FileStore::new(
            db.clone(),
            directory.clone(),
        ))
        .data(crate::pomodoro::Service::new(
            db.clone(),
            prefs.clone(),
            events.clone(),
        ))
        .data(events.clone())
        .data(crate::image_editor::Updates::new(events.clone()))
        .data(prefs)
        .data(crate::feeds::SyncService::new(
            db.clone(),
            events,
            Some(std::sync::Arc::new(crate::feeds::FeedAssets {
                db: db.clone(),
                directory: directory.clone(),
            })),
        ))
        .data(std::sync::Arc::new(crate::feeds::FeedAssets {
            db,
            directory,
        }))
        .finish()
}
