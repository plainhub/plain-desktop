//! async-graphql schema: output types, QueryRoot, MutationRoot.

mod app;
mod audio_queue;
mod bookmark;
mod chat_channel;
pub(crate) mod chat_message;
mod chat_peer;
mod chat_query;
mod datastore;
mod db;
mod discover;
mod download;
mod favorite_folder;
mod file_query;
mod file_upload;
mod logs;
mod pairing;
mod stub;
mod tag;
pub mod types;
mod util;

#[cfg(test)]
#[path = "../../../../tests/unit/local/graphql/schema/sdl.rs"]
mod sdl_tests;

use async_graphql::{EmptySubscription, MergedObject, Schema};

use app::{AppMutation, AppQuery};
use audio_queue::{AudioQueueMutation, AudioQueueQuery};
use bookmark::{BookmarkMutation, BookmarkQuery};
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
use logs::{LogsMutation, LogsQuery};
use pairing::PairingMutation;
use stub::StubQuery;
use tag::{TagMutation, TagQuery};

#[derive(MergedObject, Default)]
pub struct QueryRoot(
    AppQuery,
    AudioQueueQuery,
    BookmarkQuery,
    ChatQuery,
    LogsQuery,
    DataStoreQuery,
    DbQuery,
    FileUploadQuery,
    FavoriteFolderQuery,
    FileInfoQuery,
    TagQuery,
    DiscoverQuery,
    StubQuery,
);

#[derive(MergedObject, Default)]
pub struct MutationRoot(
    AppMutation,
    AudioQueueMutation,
    BookmarkMutation,
    ChatMessageMutation,
    ChatChannelMutation,
    ChatPeerMutation,
    LogsMutation,
    DataStoreMutation,
    DbMutation,
    FileUploadMutation,
    DiscoverMutation,
    PairingMutation,
    DownloadMutation,
    FavoriteFolderMutation,
    TagMutation,
);

pub type LocalSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema() -> LocalSchema {
    Schema::build(
        QueryRoot::default(),
        MutationRoot::default(),
        EmptySubscription,
    )
    .finish()
}
