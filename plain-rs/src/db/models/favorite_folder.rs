#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteFolderRow {
    pub root_path: String,
    pub relative_path: String,
    pub alias: Option<String>,
}
