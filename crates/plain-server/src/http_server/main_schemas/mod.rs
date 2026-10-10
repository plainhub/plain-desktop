//! async-graphql schema: output types, QueryRoot, MutationRoot.

mod app;
mod app_file;
mod app_logs;
mod audio;
mod bookmark;
mod capability;
pub mod capability_types;
mod chat_channel;
pub mod chat_message;
mod chat_peer;
mod chat_query;
mod content_common;
mod db;
mod discover;
mod download;
mod favorite_folder;
mod feed;
mod file_query;
mod file_upload;
mod image_editor_project;
pub mod media;
mod note;
mod pairing;
mod pomodoro;
mod prefs;
pub mod types;
mod util;

use async_graphql::{EmptySubscription, MergedObject, Schema};

use app::{AppMutation, AppQuery};
use app_file::AppFileQuery;
use app_logs::{AppLogsMutation, AppLogsQuery};
use audio::{AudioMutation, AudioQuery};
use bookmark::{BookmarkMutation, BookmarkQuery};
use capability::{CapabilityMutation, CapabilityQuery};
use chat_channel::ChatChannelMutation;
use chat_message::ChatMessageMutation;
use chat_peer::ChatPeerMutation;
use chat_query::ChatQuery;
use db::{DbMutation, DbQuery};
use discover::{DiscoverMutation, DiscoverQuery};
use download::DownloadMutation;
use favorite_folder::{FavoriteFolderMutation, FavoriteFolderQuery};
use feed::{FeedMutation, FeedQuery};
use file_query::FileInfoQuery;
use file_upload::{FileUploadMutation, FileUploadQuery};
use image_editor_project::{ImageEditorProjectMutation, ImageEditorProjectQuery};
use note::{NoteMutation, NoteQuery};
use pairing::PairingMutation;
use pomodoro::{PomodoroMutation, PomodoroQuery};
use prefs::{PrefsMutation, PrefsQuery};

#[derive(MergedObject, Default)]
pub struct QueryRoot(
    AppQuery,
    AudioQuery,
    BookmarkQuery,
    CapabilityQuery,
    ChatQuery,
    AppFileQuery,
    AppLogsQuery,
    PrefsQuery,
    DbQuery,
    FileUploadQuery,
    FavoriteFolderQuery,
    ImageEditorProjectQuery,
    FileInfoQuery,
    DiscoverQuery,
    NoteQuery,
    FeedQuery,
    PomodoroQuery,
    media::MediaQueryRoot,
);

#[derive(MergedObject, Default)]
pub struct MutationRoot(
    AppMutation,
    AudioMutation,
    BookmarkMutation,
    CapabilityMutation,
    ChatMessageMutation,
    ChatChannelMutation,
    ChatPeerMutation,
    AppLogsMutation,
    PrefsMutation,
    DbMutation,
    FileUploadMutation,
    DiscoverMutation,
    PairingMutation,
    DownloadMutation,
    FavoriteFolderMutation,
    ImageEditorProjectMutation,
    NoteMutation,
    FeedMutation,
    PomodoroMutation,
    media::MediaMutationRoot,
);

pub type ApiSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema() -> ApiSchema {
    Schema::build(
        QueryRoot::default(),
        MutationRoot::default(),
        EmptySubscription,
    )
    .finish()
}

#[cfg(test)]
#[path = "../../../tests/unit/api/schema.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/api/schema_split.rs"]
mod schema_split;

mod bookmark_types;
