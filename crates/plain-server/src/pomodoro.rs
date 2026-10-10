use crate::{
    content_types::Instant,
    db::Db,
    prefs::Prefs,
    ws_event::{WS_POMODORO_ACTION, WsEvent},
};
use async_graphql::{Enum, SimpleObject};
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone, Deserialize, Serialize, SimpleObject)]
#[serde(default, rename_all = "camelCase")]
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
#[derive(Enum, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Debug)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PomodoroState {
    Work,
    ShortBreak,
    LongBreak,
}
#[derive(Clone, SimpleObject)]
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
#[derive(SimpleObject)]
pub struct PomodoroTickResult {
    pub today: PomodoroToday,
    pub completed_state: Option<PomodoroState>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Runtime {
    calendar_date: String,
    day_start: DateTime<Utc>,
    round: i32,
    remaining: i32,
    total: i32,
    deadline: Option<DateTime<Utc>>,
    paused: bool,
    state: PomodoroState,
}
impl Runtime {
    fn remaining(&self, now: DateTime<Utc>) -> i32 {
        self.deadline
            .map(|d| ((d - now).num_milliseconds().max(0) + 999) / 1000)
            .map(|n| n.min(i32::MAX as i64) as i32)
            .unwrap_or(self.remaining)
    }
}
pub struct Service {
    db: Arc<Db>,
    prefs: Arc<Prefs>,
    events: broadcast::Sender<WsEvent>,
}
impl Service {
    pub fn new(db: Arc<Db>, prefs: Arc<Prefs>, events: broadcast::Sender<WsEvent>) -> Arc<Self> {
        Arc::new(Self { db, prefs, events })
    }
    pub fn settings(&self) -> anyhow::Result<PomodoroSettings> {
        let raw = self
            .prefs
            .get_user_or::<String>("pomodoro_settings", String::new());
        let settings = if raw.is_empty() {
            PomodoroSettings::default()
        } else {
            serde_json::from_str(&raw)?
        };
        anyhow::ensure!(
            (1..=1440).contains(&settings.work_duration_min)
                && (1..=1440).contains(&settings.short_break_duration_min)
                && (1..=1440).contains(&settings.long_break_duration_min)
                && (1..=100).contains(&settings.pomodoros_before_long_break),
            "invalid pomodoro settings"
        );
        Ok(settings)
    }
    fn duration(settings: &PomodoroSettings, state: PomodoroState) -> i32 {
        (match state {
            PomodoroState::Work => settings.work_duration_min,
            PomodoroState::ShortBreak => settings.short_break_duration_min,
            PomodoroState::LongBreak => settings.long_break_duration_min,
        }) * 60
    }
    fn load(conn: &rusqlite::Connection, settings: &PomodoroSettings) -> anyhow::Result<Runtime> {
        let raw: Option<String> = conn
            .query_row("SELECT data FROM pomodoro_runtime WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(raw) = raw {
            return Ok(serde_json::from_str(&raw)?);
        }
        let today = chrono::Local::now().date_naive();
        let day_start = today
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .ok_or_else(|| anyhow::anyhow!("invalid local day"))?
            .with_timezone(&Utc);
        Ok(Runtime {
            calendar_date: today.to_string(),
            day_start,
            round: 1,
            remaining: settings.work_duration_min * 60,
            total: settings.work_duration_min * 60,
            deadline: None,
            paused: false,
            state: PomodoroState::Work,
        })
    }
    fn save(conn: &rusqlite::Connection, runtime: &Runtime) -> anyhow::Result<()> {
        conn.execute("INSERT INTO pomodoro_runtime(id,data) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",[serde_json::to_string(runtime)?])?;
        Ok(())
    }
    fn snapshot(
        conn: &rusqlite::Connection,
        runtime: &Runtime,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PomodoroToday> {
        let count: Option<i32> = conn
            .query_row(
                "SELECT completed_count FROM pomodoro_items WHERE date=?1",
                [&runtime.calendar_date],
                |r| r.get(0),
            )
            .optional()?;
        Ok(PomodoroToday {
            date: Instant(runtime.day_start),
            completed_count: count.unwrap_or(0),
            current_round: runtime.round,
            time_left_sec: runtime.remaining(now),
            total_time_sec: runtime.total,
            is_running: runtime.deadline.is_some(),
            is_paused: runtime.paused,
            state: runtime.state,
        })
    }
    pub fn today(&self) -> anyhow::Result<PomodoroToday> {
        let settings = self.settings()?;
        self.db
            .with_conn(|c| Self::snapshot(c, &Self::load(c, &settings)?, Utc::now()))
    }
    pub fn configure_day(&self, date: &str, start: DateTime<Utc>) -> anyhow::Result<PomodoroToday> {
        let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
        anyhow::ensure!(parsed.to_string() == date, "invalid calendar date");
        let settings = self.settings()?;
        self.db.with_conn(|c| {
            let mut r = Self::load(c, &settings)?;
            if r.calendar_date != date {
                r.round = 1;
            }
            r.calendar_date = date.into();
            r.day_start = start;
            if r.deadline.is_none() && !r.paused {
                r.remaining = Self::duration(&settings, r.state);
                r.total = r.remaining;
            }
            Self::save(c, &r)?;
            Self::snapshot(c, &r, Utc::now())
        })
    }
    pub fn command(&self, action: &str, duration: Option<i32>) -> anyhow::Result<PomodoroToday> {
        let settings = self.settings()?;
        let now = Utc::now();
        let today = self.db.with_conn(|c| {
            let mut r = Self::load(c, &settings)?;
            match action {
                "start" => {
                    let seconds = duration.unwrap_or(r.remaining(now));
                    anyhow::ensure!((1..=86400).contains(&seconds), "invalid duration");
                    r.remaining = seconds;
                    r.deadline = Some(now + chrono::Duration::seconds(seconds as i64));
                    r.paused = false;
                }
                "pause" => {
                    r.remaining = r.remaining(now);
                    r.deadline = None;
                    r.paused = true;
                }
                "stop" => {
                    r.state = PomodoroState::Work;
                    r.remaining = Self::duration(&settings, r.state);
                    r.total = r.remaining;
                    r.deadline = None;
                    r.paused = false;
                }
                "adjust" => {
                    let seconds = duration.ok_or_else(|| anyhow::anyhow!("duration required"))?;
                    anyhow::ensure!((0..=r.total).contains(&seconds), "invalid duration");
                    r.remaining = seconds;
                    if r.deadline.is_some() {
                        r.deadline = Some(now + chrono::Duration::seconds(seconds as i64));
                    }
                }
                _ => anyhow::bail!("unknown pomodoro action"),
            }
            Self::save(c, &r)?;
            Self::snapshot(c, &r, now)
        })?;
        self.publish(action, &today, None);
        Ok(today)
    }
    pub fn tick(&self, skip: bool) -> anyhow::Result<PomodoroTickResult> {
        let settings = self.settings()?;
        let now = Utc::now();
        let result=self.db.with_conn(|c| {let tx=c.unchecked_transaction()?;let mut r=Self::load(&tx,&settings)?;
            let completed= if skip || (r.deadline.is_some() && r.remaining(now)==0) {Some(r.state)} else {None};
            if let Some(state)=completed {
                let count: Option<i32>=tx.query_row("SELECT completed_count FROM pomodoro_items WHERE date=?1",[&r.calendar_date],|row|row.get(0)).optional()?;
                let mut count=count.unwrap_or(0);
                if !skip {
                    let work=if state==PomodoroState::Work {r.total} else {0};
                    let rest=if state!=PomodoroState::Work {r.total} else {0};
                    let increment=i32::from(state==PomodoroState::Work);
                    let existing:Option<String>=tx.query_row("SELECT id FROM pomodoro_items WHERE date=?1",[&r.calendar_date],|row|row.get(0)).optional()?;
                    if let Some(id)=existing {tx.execute("UPDATE pomodoro_items SET completed_count=completed_count+?1,total_work_seconds=total_work_seconds+?2,total_break_seconds=total_break_seconds+?3,updated_at=?4 WHERE id=?5",params![increment,work,rest,now.to_rfc3339(),id])?;}
                    else {tx.execute("INSERT INTO pomodoro_items(id,date,completed_count,total_work_seconds,total_break_seconds,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",params![uuid::Uuid::new_v4().to_string(),r.calendar_date,increment,work,rest,now.to_rfc3339()])?;}
                    count+=increment;
                }
                r.state=if state==PomodoroState::Work {if count>0 && count%settings.pomodoros_before_long_break==0 {PomodoroState::LongBreak} else {PomodoroState::ShortBreak}} else {r.round+=1;PomodoroState::Work};
                r.deadline=None;r.paused=false;r.remaining=Self::duration(&settings,r.state);r.total=r.remaining;Self::save(&tx,&r)?;
            }
            let today=Self::snapshot(&tx,&r,now)?;tx.commit()?;Ok::<_,anyhow::Error>(PomodoroTickResult {today,completed_state:completed}) })?;
        if result.completed_state.is_some() {
            self.publish(
                if skip { "skip" } else { "complete" },
                &result.today,
                if skip { None } else { result.completed_state },
            );
        }
        Ok(result)
    }
    fn publish(&self, action: &str, today: &PomodoroToday, completed_state: Option<PomodoroState>) {
        let payload = serde_json::json!({"action":action,"timeLeftSec":today.time_left_sec,"totalTimeSec":today.total_time_sec,"completedCount":today.completed_count,"round":today.current_round,"state":today.state,"completedState":completed_state});
        let _ = self
            .events
            .send(WsEvent::broadcast(WS_POMODORO_ACTION, payload.to_string()));
    }
}
