use async_graphql::SimpleObject;

#[derive(SimpleObject, Clone, Debug)]
pub struct FavoriteFolder {
    pub root_path: String,
    pub full_path: String,
    pub alias: Option<String>,
}
