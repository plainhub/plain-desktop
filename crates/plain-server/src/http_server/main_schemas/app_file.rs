use async_graphql::{ComplexObject, Context, Object, SimpleObject};
use std::sync::Arc;

use super::types::parse_instant;
use super::types::{Instant, Long};
use crate::chat::app_file_store::{display_name, file_name_map};
use crate::db::{DAppFile, Db};

#[derive(SimpleObject)]
#[graphql(name = "AppFile")]
#[graphql(complex)]
pub struct AppFile {
    pub id: String,
    pub size: Long,
    pub mime_type: String,
    pub real_path: String,
    pub file_name: String,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

#[ComplexObject]
impl AppFile {
    async fn created_at(&self) -> async_graphql::Result<Instant> {
        parse_instant(&self.created_at)
    }

    async fn updated_at(&self) -> async_graphql::Result<Instant> {
        parse_instant(&self.updated_at)
    }
}

impl AppFile {
    pub fn from_dappfile(f: DAppFile, file_name: String) -> Self {
        Self {
            id: f.id,
            size: Long(f.size),
            mime_type: f.mime_type,
            real_path: f.real_path,
            file_name,
            created_at: f.created_at,
            updated_at: f.updated_at,
        }
    }
}

#[derive(Default)]
pub struct AppFileQuery;

#[Object]
impl AppFileQuery {
    async fn app_files(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AppFile>> {
        if offset < 0 || limit < 0 {
            return Err(async_graphql::Error::new("invalid pagination"));
        }
        let db = ctx.data::<Arc<Db>>()?;
        let files = db.app_file_items(offset, limit, &query)?;
        let names = file_name_map(&db.get_all_chats());
        Ok(files
            .into_iter()
            .map(|file| {
                let name = display_name(&file, &names);
                AppFile::from_dappfile(file, name)
            })
            .collect())
    }

    async fn app_file_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        Ok(ctx.data::<Arc<Db>>()?.app_file_count(&query)?)
    }
}
