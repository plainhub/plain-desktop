use async_graphql::{Context, Object};
use std::sync::Arc;

use super::util::read_log_lines;
use crate::api::context::AppCtx;

#[derive(Default)]
pub struct AppLogsQuery;

#[Object]
impl AppLogsQuery {
    async fn app_logs(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
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
