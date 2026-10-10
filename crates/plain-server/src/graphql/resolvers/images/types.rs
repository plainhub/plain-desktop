use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::Enum;
use async_graphql::ID;
use async_graphql::SimpleObject;

/// AI image-search capability status. No CLIP model runtime ships here,
/// so this always reports `UNAVAILABLE`; the field exists so clients
/// probing the phone-style AI search surface get a well-formed answer
/// instead of a GraphQL error.
#[derive(SimpleObject, Clone, Debug)]
pub struct ImageSearchStatus {
    pub status: ImageSearchStatusType,
    pub download_progress: i32,
    pub error_message: String,
    pub model_size: Long,
    pub model_dir: String,
    pub is_indexing: bool,
    pub total_images: i32,
    pub indexed_images: i32,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImageSearchStatusType {
    Unavailable,
    Downloading,
    Loading,
    Ready,
    Error,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Image {
    pub id: ID,
    pub title: String,
    pub path: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: ID,
    /// Capture time; no EXIF extraction yet, so it mirrors the file mtime.
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
