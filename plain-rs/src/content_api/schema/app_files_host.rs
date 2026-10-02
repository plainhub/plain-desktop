use super::types::{Instant, Long, parse_instant};
use crate::{
    app_files::FileStore,
    db::{DAppFile, Db},
};
use async_graphql::{Context, Object, SimpleObject};
use std::sync::Arc;

#[derive(SimpleObject)]
pub struct AppFileRecord {
    file_name: String,
    id: String,
    size: Long,
    mime_type: String,
    real_path: String,
    ref_count: i32,
    weak_hash: String,
    created_at: Instant,
    updated_at: Instant,
}
fn record(file: DAppFile) -> async_graphql::Result<AppFileRecord> {
    let name = crate::chat::app_file_store::display_name(&file, &std::collections::HashMap::new());
    Ok(AppFileRecord {
        file_name: name,
        id: file.id,
        size: Long(file.size),
        mime_type: file.mime_type,
        real_path: file.real_path,
        ref_count: file.ref_count,
        weak_hash: file.weak_hash,
        created_at: parse_instant(&file.created_at)?,
        updated_at: parse_instant(&file.updated_at)?,
    })
}
#[derive(Default)]
pub struct AppFileHostQuery;
#[Object]
impl AppFileHostQuery {
    async fn app_file_record(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<AppFileRecord>> {
        ctx.data::<Arc<Db>>()?
            .app_file_get(&id)?
            .map(record)
            .transpose()
    }
    async fn app_file_records(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AppFileRecord>> {
        if offset < 0 || limit < 0 {
            return Err(async_graphql::Error::new("invalid pagination"));
        }
        let db = ctx.data::<Arc<Db>>()?;
        let names = crate::chat::app_file_store::file_name_map(&db.get_all_chats());
        db.app_file_items(offset, limit, &query)?
            .into_iter()
            .map(|file| {
                let name = crate::chat::app_file_store::display_name(&file, &names);
                let mut result = record(file)?;
                result.file_name = name;
                Ok(result)
            })
            .collect()
    }
    async fn resolve_app_file(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<String> {
        Ok(ctx
            .data::<Arc<FileStore>>()?
            .resolve(&id)
            .map_err(async_graphql::Error::new)?
            .to_string_lossy()
            .into_owned())
    }
}
#[derive(Default)]
pub struct AppFileHostMutation;
#[Object]
impl AppFileHostMutation {
    async fn import_app_file(
        &self,
        ctx: &Context<'_>,
        source: String,
        file_name: String,
        mime_type: String,
        delete_source: bool,
    ) -> async_graphql::Result<AppFileRecord> {
        let file = ctx
            .data::<Arc<FileStore>>()?
            .import(source.into(), file_name, mime_type, delete_source)
            .await
            .map_err(async_graphql::Error::new)?;
        record(file)
    }
    async fn release_app_file(&self, ctx: &Context<'_>, id: String) -> async_graphql::Result<bool> {
        ctx.data::<Arc<FileStore>>()?
            .release(id)
            .await
            .map_err(async_graphql::Error::new)
    }
}
