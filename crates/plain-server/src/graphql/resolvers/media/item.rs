use crate::graphql::resolvers::{
    audio::public::Audio, docs::public::Doc, images::public::Image, videos::public::Video,
};
use crate::graphql::types::{Instant, Long};
use async_graphql::{ID, Interface};

#[derive(Interface)]
#[graphql(
    field(name = "id", ty = "&ID"),
    field(name = "title", ty = "&String"),
    field(name = "path", ty = "&String"),
    field(name = "size", ty = "&Long"),
    field(name = "bucketId", method = "bucket_id", ty = "&ID"),
    field(name = "createdAt", method = "created_at", ty = "&Instant"),
    field(name = "updatedAt", method = "updated_at", ty = "&Instant")
)]
pub enum MediaItem {
    Audio(Audio),
    Image(Image),
    Video(Video),
    Doc(Doc),
}
