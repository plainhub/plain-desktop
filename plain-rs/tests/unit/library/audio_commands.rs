use super::*;
use crate::library::audio_queue::NoLibrary;
struct Fake {
    commands: Vec<(Action, String, i64)>,
    fail: bool,
}
impl LibraryTracks for Fake {
    fn library_count(&mut self) -> LibraryResult<usize> {
        NoLibrary.library_count()
    }
    fn library_path_at(&mut self, o: usize, s: &str) -> LibraryResult<Option<String>> {
        NoLibrary.library_path_at(o, s)
    }
    fn library_tracks_page(
        &mut self,
        o: usize,
        l: usize,
        s: &str,
    ) -> LibraryResult<Vec<AudioTrack>> {
        NoLibrary.library_tracks_page(o, l, s)
    }
    fn library_locate(&mut self, p: &str, s: &str) -> LibraryResult<i64> {
        NoLibrary.library_locate(p, s)
    }
    fn library_contains(&mut self, p: &str) -> LibraryResult<bool> {
        NoLibrary.library_contains(p)
    }
}
impl Engine for Fake {
    fn metadata(&mut self, path: &str) -> LibraryResult<AudioTrack> {
        Ok(AudioTrack::from_path_stem(path))
    }
    fn execute(&mut self, c: &EngineCommand) -> LibraryResult<EngineReport> {
        if self.fail {
            return Err(LibraryError::Other("player unavailable".into()));
        }
        self.commands
            .push((c.action, c.playback.path.clone(), c.playback.position_ms));
        Ok(EngineReport {
            path: c.playback.path.clone(),
            position_ms: c.playback.position_ms,
            revision: c.playback.revision,
            loaded: c.new_track,
        })
    }
}
#[test]
fn commands_preserve_resume_and_repeat_one_only_applies_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    let mut engine = Fake {
        commands: vec![],
        fail: false,
    };
    play_path(&db, &prefs, &mut engine, "one").unwrap();
    audio_queue::enqueue(&db, &[AudioTrack::from_path_stem("two")], false).unwrap();
    command(&db, &prefs, &mut engine, Action::Seek, 3_000_000_123, 1.0).unwrap();
    command(&db, &prefs, &mut engine, Action::Pause, 0, 1.0).unwrap();
    let resumed = command(&db, &prefs, &mut engine, Action::Play, 0, 1.0).unwrap();
    assert_eq!(resumed.position_ms, 3_000_000_123);
    audio_queue::save_audio_mode(&prefs, "REPEAT_ONE").unwrap();
    let repeated = command(&db, &prefs, &mut engine, Action::Completed, 0, 1.0).unwrap();
    assert_eq!(repeated.path, "one");
    assert_eq!(repeated.position_ms, 0);
    let next = command(&db, &prefs, &mut engine, Action::Next, 0, 1.0).unwrap();
    assert_eq!(next.path, "two");
    assert!(
        audio_queue::history_page(&db, 0, 10, "")
            .unwrap()
            .is_empty()
    );
    assert!(command(&db, &prefs, &mut engine, Action::SetSpeed, 0, f32::NAN).is_err());
    engine.fail = true;
    assert!(command(&db, &prefs, &mut engine, Action::Pause, 0, 1.0).is_err());
}
