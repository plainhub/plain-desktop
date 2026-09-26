//! Resolver-mapping tests for the audio-queue schema module — the queue
//! state-machine behavior locks live in plain-rs; these cover the local
//! seam (NoLibrary library source, mode mapping, DSL text extraction).
use super::*;
use plain_rs::library::audio_queue::AudioTrack;

fn test_library() -> LibraryDb {
    let dir = std::env::temp_dir().join(format!("plain-desktop-aqrs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    LibraryDb::open(&dir.join("library.db")).unwrap()
}

#[test]
fn mode_maps_names_with_repeat_default() {
    let db = test_library();
    assert!(matches!(media_play_mode_of(&db), MediaPlayMode::Repeat));
    plain_rs::library::audio_queue::save_audio_mode(&db, "SHUFFLE");
    assert!(matches!(media_play_mode_of(&db), MediaPlayMode::Shuffle));
    plain_rs::library::audio_queue::save_audio_mode(&db, "REPEAT_ONE");
    assert!(matches!(media_play_mode_of(&db), MediaPlayMode::RepeatOne));
}

#[test]
fn text_of_extracts_the_dsl_text_field() {
    assert_eq!(text_of(r#"text:"road song" trash:false"#), "road song");
    assert_eq!(text_of(""), "");
}

/// playAudio → queue + history through the core, on the exact handles
/// the resolvers use (queue renders head→manual→tail, supersede hidden).
#[test]
fn play_audio_flow_on_the_resolver_seam() {
    let db = test_library();
    let track = AudioTrack::from_path_stem("/music/My Song.mp3");
    plain_rs::library::audio_queue::enqueue(&db, std::slice::from_ref(&track), false);
    plain_rs::library::audio_queue::on_playing(&db, &track.path, &track.title, &track.artist, 0);

    let mut lib = plain_rs::library::audio_queue::NoLibrary;
    let page = plain_rs::library::audio_queue::queue_page(&db, &mut lib, 0, 10, "").unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].title, "My Song");
    // playAllAudios on an empty library returns null (documented local
    // semantics — no media index yet).
    assert!(
        plain_rs::library::audio_queue::set_library_source(&db, &mut lib, None, false)
            .unwrap()
            .is_none()
    );
}
