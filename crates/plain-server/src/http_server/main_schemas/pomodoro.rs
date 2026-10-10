use crate::pomodoro::Service;
pub use crate::pomodoro::{PomodoroSettings, PomodoroToday};
use async_graphql::{Context, Object};
use std::sync::Arc;
#[derive(Default)]
pub struct PomodoroQuery;
#[Object]
impl PomodoroQuery {
    async fn pomodoro_settings(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<PomodoroSettings> {
        Ok(ctx.data::<Arc<Service>>()?.settings()?)
    }
    async fn pomodoro_today(&self, ctx: &Context<'_>) -> async_graphql::Result<PomodoroToday> {
        Ok(ctx.data::<Arc<Service>>()?.today()?)
    }
}
#[derive(Default)]
pub struct PomodoroMutation;
#[Object]
impl PomodoroMutation {
    async fn start_pomodoro(
        &self,
        ctx: &Context<'_>,
        duration_sec: i32,
    ) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Service>>()?
            .command("start", Some(duration_sec))?;
        Ok(true)
    }
    async fn pause_pomodoro(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Service>>()?.command("pause", None)?;
        Ok(true)
    }
    async fn stop_pomodoro(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Service>>()?.command("stop", None)?;
        Ok(true)
    }
}
