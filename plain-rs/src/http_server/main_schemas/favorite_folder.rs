//! Favorite folders (file-browser pins) — backed by the shared plain-rs
//! library core (`the shared SQLite database`), same behavior as the NAS server.
//! The mutations return the whole updated list (phone contract).

use crate::library::favorite_folders;
use async_graphql::{Context, Object, Result};
use std::sync::Arc;

use crate::content_types::FavoriteFolder;
use crate::db::Db;

fn to_gql(f: favorite_folders::FavoriteFolder) -> FavoriteFolder {
    FavoriteFolder {
        full_path: favorite_folders::full_path_of(&f),
        root_path: f.root_path,
        alias: f.alias,
    }
}

fn list_gql(db: &Arc<Db>) -> Result<Vec<FavoriteFolder>> {
    Ok(favorite_folders::list(db)?
        .into_iter()
        .map(to_gql)
        .collect())
}

#[derive(Default)]
pub struct FavoriteFolderQuery;

#[Object]
impl FavoriteFolderQuery {
    /// List all favorite folders.
    async fn favorite_folders(&self, ctx: &Context<'_>) -> Result<Vec<FavoriteFolder>> {
        let db = ctx.data::<Arc<Db>>()?;
        list_gql(db)
    }
}

#[derive(Default)]
pub struct FavoriteFolderMutation;

#[Object]
impl FavoriteFolderMutation {
    /// Register a favorite folder and return the whole list.
    async fn add_favorite_folder(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "rootPath")] root_path: String,
        #[graphql(name = "fullPath")] full_path: String,
    ) -> Result<Vec<FavoriteFolder>> {
        let db = ctx.data::<Arc<Db>>()?;
        favorite_folders::add_full_path(db, &root_path, &full_path)?;
        list_gql(db)
    }

    /// Remove the favorite folder identified by `fullPath` and return the
    /// whole list.
    async fn remove_favorite_folder(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fullPath")] full_path: String,
    ) -> Result<Vec<FavoriteFolder>> {
        let db = ctx.data::<Arc<Db>>()?;
        if let Some(f) = favorite_folders::find_by_full_path(db, &full_path)? {
            favorite_folders::remove(db, &f.root_path, &f.relative_path)?;
        }
        list_gql(db)
    }

    /// Set the favorite folder's display alias and return the whole list.
    async fn set_favorite_folder_alias(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fullPath")] full_path: String,
        alias: String,
    ) -> Result<Vec<FavoriteFolder>> {
        let db = ctx.data::<Arc<Db>>()?;
        if let Some(f) = favorite_folders::find_by_full_path(db, &full_path)? {
            favorite_folders::set_alias(db, &f.root_path, &f.relative_path, &alias)?;
        }
        list_gql(db)
    }
}
