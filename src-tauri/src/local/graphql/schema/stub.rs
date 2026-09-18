use async_graphql::Object;

use super::types::{FavoriteFolder, Mount};

#[derive(Default)]
pub struct StubQuery;

#[Object]
impl StubQuery {
    async fn sms_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn contact_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn call_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn image_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn audio_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn video_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn package_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn note_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn doc_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn feed_entry_count(&self, _query: Option<String>) -> i32 {
        0
    }
    async fn mounts(&self) -> Vec<Mount> {
        vec![]
    }
    async fn favorite_folders(&self) -> Vec<FavoriteFolder> {
        vec![]
    }
}
