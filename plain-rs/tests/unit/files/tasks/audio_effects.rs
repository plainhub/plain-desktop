use super::*;
use crate::error::DomainError;
use crate::media::audio::commands::*;
use crate::media::audio::queue::AudioTrack;
use crate::media::audio::queue::LibraryTracks;
use crate::media::audio::queue::NoLibrary;
struct Fake {
    commands: Vec<(Action, String, i64)>,
    fail: bool,
}
impl LibraryTracks for Fake {
    fn library_count(&mut self) -> DomainResult<usize> {
        NoLibrary.library_count()
    }
    fn library_path_at(&mut self, o: usize, s: &str) -> DomainResult<Option<String>> {
        NoLibrary.library_path_at(o, s)
    }
    fn library_tracks_page(
        &mut self,
        o: usize,
        l: usize,
        s: &str,
    ) -> DomainResult<Vec<AudioTrack>> {
        NoLibrary.library_tracks_page(o, l, s)
    }
    fn library_locate(&mut self, p: &str, s: &str) -> DomainResult<i64> {
        NoLibrary.library_locate(p, s)
    }
    fn library_contains(&mut self, p: &str) -> DomainResult<bool> {
        NoLibrary.library_contains(p)
    }
}
impl Engine for Fake {
    fn metadata(&mut self, path: &str) -> DomainResult<AudioTrack> {
        Ok(AudioTrack::from_path_stem(path))
    }
    fn execute(&mut self, c: &EngineCommand) -> DomainResult<EngineReport> {
        if self.fail {
            return Err(DomainError::Other("player unavailable".into()));
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
fn failed_engine_stop_is_retried_after_restart_and_does_not_interrupt_new_playback() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("db");
    let db = Db::open(&db_path).unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs")).unwrap();
    let mut engine = Fake {
        commands: vec![],
        fail: false,
    };
    play_path(&db, &prefs, &mut engine, "moved").unwrap();
    engine.fail = true;
    assert!(clear_after_move(&db, &prefs, &mut engine, Some("receipt"), Some("moved")).is_err());
    assert!(audio_playback::snapshot(&db).unwrap().path.is_empty());
    drop(db);
    let db = Db::open(&db_path).unwrap();
    engine.fail = false;
    clear_after_move(&db, &prefs, &mut engine, Some("receipt"), None).unwrap();
    assert!(matches!(engine.commands.last().unwrap().0, Action::Clear));
    let count = engine.commands.len();
    clear_after_move(&db, &prefs, &mut engine, Some("receipt"), None).unwrap();
    assert_eq!(engine.commands.len(), count);
    play_path(&db, &prefs, &mut engine, "old").unwrap();
    engine.fail = true;
    assert!(clear_after_move(&db, &prefs, &mut engine, Some("second"), Some("old")).is_err());
    engine.fail = false;
    play_path(&db, &prefs, &mut engine, "new").unwrap();
    let count = engine.commands.len();
    clear_after_move(&db, &prefs, &mut engine, Some("second"), None).unwrap();
    assert_eq!(audio_playback::snapshot(&db).unwrap().path, "new");
    assert_eq!(engine.commands.len(), count);
    engine.fail = true;
    assert!(clear_after_move(&db, &prefs, &mut engine, Some("third"), Some("new")).is_err());
    engine.fail = false;
    play_path(&db, &prefs, &mut engine, "new").unwrap();
    let count = engine.commands.len();
    clear_after_move(&db, &prefs, &mut engine, Some("third"), None).unwrap();
    assert_eq!(audio_playback::snapshot(&db).unwrap().path, "new");
    assert_eq!(engine.commands.len(), count);
}
