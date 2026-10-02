#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FavoriteFolderRow {
    pub root_path: String,
    pub relative_path: String,
    pub alias: Option<String>,
}
