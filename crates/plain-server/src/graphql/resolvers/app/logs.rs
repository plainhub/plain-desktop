use async_graphql::{Context, Object};
use std::sync::Arc;

use crate::assembly::context::AppCtx;
use crate::graphql::resolvers::common::util::read_log_lines;

#[derive(Default)]
pub struct AppLogsQuery;

#[Object]
impl AppLogsQuery {
    async fn app_logs(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> Vec<String> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        read_log_lines(&c.log_dir.join("plain.log"), &query, offset, limit)
    }

    async fn app_log_path(&self, ctx: &Context<'_>) -> String {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.log_dir.join("plain.log").to_string_lossy().into_owned()
    }
}

#[derive(Default)]
pub struct AppLogsMutation;

#[Object]
impl AppLogsMutation {
    async fn clear_app_logs(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let log_file = c.log_dir.join("plain.log");
        if log_file.exists() {
            std::fs::write(&log_file, b"").is_ok()
        } else {
            true
        }
    }
}
