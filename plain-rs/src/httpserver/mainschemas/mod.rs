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
mod datastore;
mod db;
mod discover;
mod download;
mod favorite_folder;
mod file_query;
mod file_upload;
pub mod media;
mod pairing;
mod stub;
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
use datastore::{DataStoreMutation, DataStoreQuery};
use db::{DbMutation, DbQuery};
use discover::{DiscoverMutation, DiscoverQuery};
use download::DownloadMutation;
use favorite_folder::{FavoriteFolderMutation, FavoriteFolderQuery};
use file_query::FileInfoQuery;
use file_upload::{FileUploadMutation, FileUploadQuery};
use pairing::PairingMutation;
use stub::StubQuery;

#[derive(MergedObject, Default)]
pub struct QueryRoot(
    AppQuery,
    AudioQuery,
    BookmarkQuery,
    CapabilityQuery,
    ChatQuery,
    AppFileQuery,
    AppLogsQuery,
    DataStoreQuery,
    DbQuery,
    FileUploadQuery,
    FavoriteFolderQuery,
    FileInfoQuery,
    DiscoverQuery,
    StubQuery,
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
    DataStoreMutation,
    DbMutation,
    FileUploadMutation,
    DiscoverMutation,
    PairingMutation,
    DownloadMutation,
    FavoriteFolderMutation,
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
