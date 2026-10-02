/// Content-addressable file store record. Matches plain-app `DAppFile`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DAppFile {
    /// Full SHA-256 hex digest (64 chars) — primary key.
    pub id: String,
    pub size: i64,
    pub mime_type: String,
    pub real_path: String,
    pub ref_count: i32,
    /// SHA-256 hex digest of first 4 KB + last 4 KB (fast dedup probe).
    pub weak_hash: String,
    pub created_at: String,
    pub updated_at: String,
}
