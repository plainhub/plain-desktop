use crate::{
    db::Db,
    library::{
        LibraryError, LibraryResult,
        audio_playback::{self, Playback},
        audio_queue::{self, AudioTrack, LibraryTracks},
    },
    prefs::Prefs,
};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Action {
    Play,
    Pause,
    Seek,
    Next,
    Previous,
    Completed,
    Clear,
    Restart,
    SetSpeed,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCommand {
    pub action: Action,
    pub new_track: bool,
    pub track: Option<AudioTrack>,
    pub playback: Playback,
    pub speed: f32,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineReport {
    pub path: String,
    pub position_ms: i64,
    pub revision: i64,
}
pub trait Engine: LibraryTracks {
    fn metadata(&mut self, path: &str) -> LibraryResult<AudioTrack>;
    fn execute(&mut self, command: &EngineCommand) -> LibraryResult<EngineReport>;
}
pub fn play_track(
    db: &Db,
    prefs: &Prefs,
    engine: &mut impl Engine,
    track: AudioTrack,
    enqueue: bool,
) -> LibraryResult<Playback> {
    if track.path.trim().is_empty() || track.duration_ms < 0 {
        return Err(LibraryError::Other("invalid audio track".into()));
    }
    let playback = audio_playback::prepare_track(db, &track, enqueue)?;
    dispatch(
        db,
        prefs,
        engine,
        Action::Play,
        Some(track),
        playback,
        None,
        true,
    )
}
pub fn play_path(
    db: &Db,
    prefs: &Prefs,
    engine: &mut impl Engine,
    path: &str,
) -> LibraryResult<Playback> {
    if path.trim().is_empty() {
        return Err(LibraryError::Other("empty audio path".into()));
    }
    let track = engine.metadata(path)?;
    if track.path != path {
        return Err(LibraryError::Other("audio metadata path mismatch".into()));
    }
    play_track(db, prefs, engine, track, true)
}
pub fn command(
    db: &Db,
    prefs: &Prefs,
    engine: &mut impl Engine,
    action: Action,
    position_ms: i64,
    speed: f32,
) -> LibraryResult<Playback> {
    let mut actual = action;
    let mut track = None;
    let playback = match action {
        Action::Next | Action::Previous | Action::Completed => {
            let mode = audio_queue::get_audio_mode(prefs)?;
            let selected = if matches!(action, Action::Completed) && mode == "REPEAT_ONE" {
                let path = audio_queue::get_audio_current(db)?;
                if path.is_empty() {
                    None
                } else {
                    Some(engine.metadata(&path)?)
                }
            } else {
                audio_queue::select_next(
                    db,
                    engine,
                    !matches!(action, Action::Previous),
                    mode == "SHUFFLE",
                )?
            };
            if let Some(selected) = selected {
                track = Some(selected);
                actual = Action::Play;
                audio_playback::seek(db, 0)?
            } else {
                actual = Action::Pause;
                audio_playback::invalidate(db)?
            }
        }
        Action::Seek => audio_playback::seek(db, position_ms)?,
        Action::Clear => {
            audio_queue::save_audio_current(db, "")?;
            audio_playback::invalidate(db)?
        }
        Action::SetSpeed => {
            if !speed.is_finite() || speed <= 0.0 {
                return Err(LibraryError::Other("invalid playback speed".into()));
            }
            prefs
                .set_user("audio_playback_speed", speed)
                .map_err(|e| LibraryError::Other(e.to_string()))?;
            audio_playback::invalidate(db)?
        }
        _ => audio_playback::invalidate(db)?,
    };
    let new_track = track.is_some();
    if matches!(actual, Action::Play | Action::Seek) && track.is_none() {
        if playback.path.is_empty() {
            return Err(LibraryError::Other("no selected audio".into()));
        }
        track = Some(engine.metadata(&playback.path)?);
    }
    dispatch(
        db,
        prefs,
        engine,
        actual,
        track,
        playback,
        matches!(action, Action::SetSpeed).then_some(speed),
        new_track,
    )
}
fn dispatch(
    db: &Db,
    prefs: &Prefs,
    engine: &mut impl Engine,
    action: Action,
    track: Option<AudioTrack>,
    playback: Playback,
    speed: Option<f32>,
    new_track: bool,
) -> LibraryResult<Playback> {
    let speed = match speed {
        Some(value) => value,
        None => prefs
            .get_user::<f32>("audio_playback_speed")
            .map_err(|e| LibraryError::Other(e.to_string()))?
            .unwrap_or(1.0),
    };
    if !speed.is_finite() || speed <= 0.0 {
        return Err(LibraryError::Other("invalid stored playback speed".into()));
    }
    if track
        .as_ref()
        .is_some_and(|t| t.path != playback.path || t.duration_ms < 0)
    {
        return Err(LibraryError::Other("invalid engine track".into()));
    }
    let revision = playback.revision;
    let selected_path = playback.path.clone();
    let report = engine.execute(&EngineCommand {
        action,
        new_track,
        track,
        playback,
        speed,
    })?;
    if report.revision != revision
        || report.position_ms < 0
        || (matches!(action, Action::Play | Action::Seek) && report.path != selected_path)
    {
        return Err(LibraryError::Other(
            "invalid audio engine acknowledgement".into(),
        ));
    }
    if !report.path.is_empty() {
        audio_playback::report(db, &report.path, revision, report.position_ms)?;
    }
    audio_playback::snapshot(db)
}

#[cfg(test)]
#[path = "../../tests/unit/library/audio_commands.rs"]
mod tests;
