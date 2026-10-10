use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Doc {
    pub id: ID,
    pub title: String,
    pub path: String,
    /// Lowercased filename extension (plain-app `getFilenameExtension`).
    pub extension: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: ID,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

/// One extension group of the docs sidebar (plain-app `DocExtGroup`).
#[derive(SimpleObject, Clone, Debug)]
pub struct DocExtGroup {
    pub ext: String,
    pub count: i32,
}
