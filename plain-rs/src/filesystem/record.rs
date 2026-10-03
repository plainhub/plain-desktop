use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub name: String,
    pub path: String,
    pub permission: String,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub size: i64,
    pub is_dir: bool,
    pub child_count: i32,
    #[serde(default)]
    pub media_id: String,
}
impl FileRecord {
    pub fn stat(path: &std::path::Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        super::file_info_to_model(path, &metadata, metadata.is_dir()).map(Self::from)
    }
}
impl From<super::FileEntry> for FileRecord {
    fn from(entry: super::FileEntry) -> Self {
        Self {
            name: std::path::Path::new(&entry.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: entry.path,
            permission: String::new(),
            created_at: Some(entry.created_at.timestamp_millis()),
            updated_at: entry.updated_at.timestamp_millis(),
            size: entry.size,
            is_dir: entry.is_dir,
            child_count: entry.child_count,
            media_id: String::new(),
        }
    }
}
