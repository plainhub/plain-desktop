use async_graphql::{ComplexObject, Context, Object, SimpleObject};
use std::sync::Arc;

use super::media::types::{Instant, Long};
use super::types::parse_instant;
use crate::api::context::AppCtx;
use crate::api::db::DAppFile;
use crate::chat::app_file_store::{display_name, file_name_map};

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
    ) -> Vec<AppFile> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let files = c.db.get_app_file_page(limit, offset);
        let name_map = file_name_map(&c.db.get_all_chats());
        let text = query.trim();
        files
            .into_iter()
            .map(|f| {
                let display = display_name(&f, &name_map);
                AppFile::from_dappfile(f, display)
            })
            .filter(|f| text.is_empty() || f.file_name.contains(text))
            .collect()
    }

    async fn app_file_count(&self, ctx: &Context<'_>, query: String) -> i32 {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let text = query.trim();
        if text.is_empty() {
            return c.db.count_app_files();
        }
        let name_map = file_name_map(&c.db.get_all_chats());
        c.db.get_all_app_files()
            .into_iter()
            .filter(|f| {
                let display = display_name(f, &name_map);
                display.contains(text)
            })
            .count() as i32
    }
}
