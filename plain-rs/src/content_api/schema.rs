pub use crate::content_types as types;
#[path = "../http_server/main_schemas/app_file.rs"]
mod app_file;
mod app_files_host;
mod audio_host;
#[path = "../http_server/main_schemas/bookmark.rs"]
mod bookmark;
#[path = "../http_server/main_schemas/bookmark_types.rs"]
mod bookmark_types;
mod bookmarks;
mod clipboard;
#[path = "../http_server/main_schemas/content_common.rs"]
mod content_common;
#[path = "../http_server/main_schemas/feed.rs"]
mod feed;
mod image_editor_host;
#[path = "../http_server/main_schemas/image_editor_project.rs"]
mod image_editor_project;
#[path = "../http_server/main_schemas/note.rs"]
mod note;
#[path = "../http_server/main_schemas/pomodoro.rs"]
mod pomodoro;
mod pomodoro_host;
mod shares_host;
mod tags;
mod video_progress;
use async_graphql::{EmptySubscription, MergedObject, Schema};
#[derive(MergedObject, Default)]
pub struct Query(
    audio_host::AudioHostQuery,
    video_progress::VideoProgressQuery,
    shares_host::ShareHostQuery,
    app_file::AppFileQuery,
    app_files_host::AppFileHostQuery,
    note::NoteQuery,
    feed::FeedQuery,
    tags::TagQuery,
    clipboard::ClipboardQuery,
    bookmark::BookmarkQuery,
    pomodoro::PomodoroQuery,
    pomodoro_host::PomodoroRecordQuery,
    image_editor_project::ImageEditorProjectQuery,
    image_editor_host::ImageEditorItemsQuery,
);
#[derive(MergedObject, Default)]
pub struct Mutation(
    audio_host::AudioHostMutation,
    video_progress::VideoProgressMutation,
    shares_host::ShareHostMutation,
    app_files_host::AppFileHostMutation,
    note::NoteMutation,
    feed::FeedMutation,
    tags::TagMutation,
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
    Schema::build(Query::default(), Mutation::default(), EmptySubscription)
        .data(std::sync::Arc::new(super::audio::Audio::new(
            db.clone(),
            host.clone(),
        )))
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
