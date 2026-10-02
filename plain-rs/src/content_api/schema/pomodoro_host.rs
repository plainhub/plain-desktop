use crate::{
    content_types::{Instant, Long},
    db::Db,
    pomodoro::{PomodoroTickResult, PomodoroToday, Service},
};
use async_graphql::{Context, ID, Object, SimpleObject};
use std::sync::Arc;
#[derive(SimpleObject)]
pub struct PomodoroRecord {
    pub id: ID,
    /// Calendar date (YYYY-MM-DD) in the host device's local timezone.
    pub date: String,
    pub completed_count: i32,
    pub total_work_sec: Long,
    pub total_break_sec: Long,
    pub created_at: Instant,
    pub updated_at: Instant,
}
#[derive(Default)]
pub struct PomodoroRecordQuery;
#[Object]
impl PomodoroRecordQuery {
    async fn pomodoro_record(
        &self,
        ctx: &Context<'_>,
        date: String,
    ) -> async_graphql::Result<Option<PomodoroRecord>> {
        ctx.data::<Arc<Db>>()?
            .pomodoro_get_by_date(&date)?
            .map(record)
            .transpose()
    }
    async fn pomodoro_records(
        &self,
        ctx: &Context<'_>,
        start_date: String,
    ) -> async_graphql::Result<Vec<PomodoroRecord>> {
        ctx.data::<Arc<Db>>()?
            .pomodoro_list()?
            .into_iter()
            .filter(|r| r.date >= start_date)
            .map(record)
            .collect()
    }
    async fn pomodoro_total_completed(&self, ctx: &Context<'_>) -> async_graphql::Result<Long> {
        Ok(Long(ctx.data::<Arc<Db>>()?.pomodoro_total_completed()?))
    }
}
fn record(r: crate::db::PomodoroItemRow) -> async_graphql::Result<PomodoroRecord> {
    Ok(PomodoroRecord {
        id: r.id.into(),
        date: r.date,
        completed_count: r.completed_count,
        total_work_sec: Long(r.total_work_seconds as i64),
        total_break_sec: Long(r.total_break_seconds as i64),
        created_at: super::content_common::instant(&r.created_at)?,
        updated_at: super::content_common::instant(&r.updated_at)?,
    })
}
#[derive(Default)]
pub struct PomodoroHostMutation;
#[Object]
impl PomodoroHostMutation {
    async fn configure_pomodoro_day(
        &self,
        ctx: &Context<'_>,
        date: String,
        day_start: Instant,
    ) -> async_graphql::Result<PomodoroToday> {
        Ok(ctx
            .data::<Arc<Service>>()?
            .configure_day(&date, day_start.0)?)
    }
    async fn tick_pomodoro(&self, ctx: &Context<'_>) -> async_graphql::Result<PomodoroTickResult> {
        Ok(ctx.data::<Arc<Service>>()?.tick(false)?)
    }
    async fn skip_pomodoro(&self, ctx: &Context<'_>) -> async_graphql::Result<PomodoroTickResult> {
        Ok(ctx.data::<Arc<Service>>()?.tick(true)?)
    }
    async fn adjust_pomodoro(
        &self,
        ctx: &Context<'_>,
        duration_sec: i32,
    ) -> async_graphql::Result<PomodoroToday> {
        Ok(ctx
            .data::<Arc<Service>>()?
            .command("adjust", Some(duration_sec))?)
    }
}
