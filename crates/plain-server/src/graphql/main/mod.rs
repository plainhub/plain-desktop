//! async-graphql schema: output types, QueryRoot, MutationRoot.

mod app;
mod app_logs;
mod audio;
mod capability;
pub mod capability_types;
mod chat_channel;
pub mod chat_message;
mod chat_peer;
mod chat_query;
mod db;
mod discover;
mod download;
mod file_query;
mod file_upload;
pub mod media;
mod pairing;
mod prefs;
pub mod types;
mod util;

use async_graphql::{EmptySubscription, MergedObject, Schema};

use crate::graphql::shared::app_file::AppFileQuery;
use crate::graphql::shared::bookmark::{BookmarkMutation, BookmarkQuery};
use crate::graphql::shared::favorite_folder::{FavoriteFolderMutation, FavoriteFolderQuery};
use crate::graphql::shared::feed::{FeedMutation, FeedQuery};
use crate::graphql::shared::image_editor_project::{
    ImageEditorProjectMutation, ImageEditorProjectQuery,
};
use crate::graphql::shared::note::{NoteMutation, NoteQuery};
use crate::graphql::shared::pomodoro::{PomodoroMutation, PomodoroQuery};
use app::{AppMutation, AppQuery};
use app_logs::{AppLogsMutation, AppLogsQuery};
use audio::{AudioMutation, AudioQuery};
use capability::{CapabilityMutation, CapabilityQuery};
use chat_channel::ChatChannelMutation;
use chat_message::ChatMessageMutation;
use chat_peer::ChatPeerMutation;
use chat_query::ChatQuery;
use db::{DbMutation, DbQuery};
use discover::{DiscoverMutation, DiscoverQuery};
use download::DownloadMutation;
use file_query::FileInfoQuery;
use file_upload::{FileUploadMutation, FileUploadQuery};
use pairing::PairingMutation;
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
#[path = "../../../tests/unit/graphql/main/schema.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/graphql/main/schema_split.rs"]
mod schema_split;
