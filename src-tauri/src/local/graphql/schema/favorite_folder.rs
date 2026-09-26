//! Favorite folders (file-browser pins) — backed by the shared plain-rs
//! library core (`local_library.db`), same behavior as the NAS server.
//! The mutations return the whole updated list (phone contract).

use async_graphql::{Context, Object};
use plain_rs::library::favorite_folders;
use std::sync::Arc;

use crate::local::graphql::context::AppCtx;
use crate::local::graphql::schema::types::FavoriteFolder;

fn to_gql(f: favorite_folders::FavoriteFolder) -> FavoriteFolder {
    FavoriteFolder {
        full_path: favorite_folders::full_path_of(&f),
        root_path: f.root_path,
        alias: f.alias,
    }
}

fn list_gql(c: &Arc<AppCtx>) -> Vec<FavoriteFolder> {
    favorite_folders::list(&c.library)
        .into_iter()
        .map(to_gql)
        .collect()
}

#[derive(Default)]
pub struct FavoriteFolderQuery;

#[Object]
impl FavoriteFolderQuery {
    /// List all favorite folders.
    async fn favorite_folders(&self, ctx: &Context<'_>) -> Vec<FavoriteFolder> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        list_gql(c)
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
    ) -> Vec<FavoriteFolder> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let (root, rel) = favorite_folders::split_full_path(&root_path, &full_path);
        favorite_folders::add(&c.library, &root, &rel);
        list_gql(c)
    }

    /// Remove the favorite folder identified by `fullPath` and return the
    /// whole list.
    async fn remove_favorite_folder(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fullPath")] full_path: String,
    ) -> Vec<FavoriteFolder> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        if let Some(f) = favorite_folders::find_by_full_path(&c.library, &full_path) {
            favorite_folders::remove(&c.library, &f.root_path, &f.relative_path);
        }
        list_gql(c)
    }

    /// Set the favorite folder's display alias and return the whole list.
    async fn set_favorite_folder_alias(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fullPath")] full_path: String,
        alias: String,
    ) -> Vec<FavoriteFolder> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        if let Some(f) = favorite_folders::find_by_full_path(&c.library, &full_path) {
            favorite_folders::set_alias(&c.library, &f.root_path, &f.relative_path, &alias);
        }
        list_gql(c)
    }
}
