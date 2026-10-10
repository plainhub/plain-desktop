use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Video {
    pub id: ID,
    pub title: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: ID,
    /// Capture time; container metadata extraction lands later, so it
    /// mirrors the file mtime for now.
    #[graphql(name = "takenAt")]
    pub taken_at: Option<Instant>,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    /// No per-media favorites on NAS — always false (plain-app parity).
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
}
