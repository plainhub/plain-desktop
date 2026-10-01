use super::types::Instant;
use async_graphql::{Context, Object, SimpleObject};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::api::context::{AppCtx, WS_POMODORO_ACTION, WsEvent};

#[derive(Clone, Deserialize, Serialize, SimpleObject)]
#[serde(default)]
pub struct PomodoroSettings {
    pub work_duration_min: i32,
    pub short_break_duration_min: i32,
    pub long_break_duration_min: i32,
    pub pomodoros_before_long_break: i32,
    pub show_notification: bool,
    pub play_sound_on_complete: bool,
    pub sound_path: String,
    pub original_sound_name: String,
}

impl Default for PomodoroSettings {
    fn default() -> Self {
        Self {
            work_duration_min: 25,
            short_break_duration_min: 5,
            long_break_duration_min: 15,
            pomodoros_before_long_break: 4,
            show_notification: true,
            play_sound_on_complete: true,
            sound_path: String::new(),
            original_sound_name: String::new(),
        }
    }
}

#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq, Serialize)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PomodoroState {
    Work,
    ShortBreak,
    LongBreak,
}

#[derive(SimpleObject)]
pub struct PomodoroToday {
    pub date: Instant,
    pub completed_count: i32,
    pub current_round: i32,
    pub time_left_sec: i32,
    pub total_time_sec: i32,
    pub is_running: bool,
    pub is_paused: bool,
    pub state: PomodoroState,
}

#[derive(Default)]
pub struct PomodoroQuery;

#[Object]
impl PomodoroQuery {
    async fn pomodoro_settings(&self, ctx: &Context<'_>) -> PomodoroSettings {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .prefs
            .get_or("pomodoro_settings", PomodoroSettings::default())
    }

    async fn pomodoro_today(&self) -> PomodoroToday {
        let today = chrono::Local::now().date_naive();
        let local_midnight = today.and_hms_opt(0, 0, 0).unwrap();
        PomodoroToday {
            date: Instant(
                local_midnight
                    .and_local_timezone(chrono::Local)
                    .single()
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            completed_count: 0,
            current_round: 1,
            time_left_sec: 0,
            total_time_sec: 0,
            is_running: false,
            is_paused: false,
            state: PomodoroState::Work,
        }
    }
}

#[derive(Default)]
pub struct PomodoroMutation;

#[Object]
impl PomodoroMutation {
    async fn start_pomodoro(&self, ctx: &Context<'_>, duration_sec: i32) -> bool {
        let event = json!({
            "action": "start",
            "timeLeftSec": duration_sec,
            "totalTimeSec": duration_sec,
            "completedCount": 0,
            "round": 1,
            "state": "WORK"
        });
        let _ = ctx
            .data_unchecked::<Arc<AppCtx>>()
            .event_tx
            .send(WsEvent::broadcast(WS_POMODORO_ACTION, event.to_string()));
        true
    }

    async fn pause_pomodoro(&self, ctx: &Context<'_>) -> bool {
        publish_action(ctx, "pause");
        true
    }

    async fn stop_pomodoro(&self, ctx: &Context<'_>) -> bool {
        publish_action(ctx, "stop");
        true
    }
}

fn publish_action(ctx: &Context<'_>, action: &str) {
    let event = json!({ "action": action });
    let _ = ctx
        .data_unchecked::<Arc<AppCtx>>()
        .event_tx
        .send(WsEvent::broadcast(WS_POMODORO_ACTION, event.to_string()));
}
