//! The event→catalog mapping a media watcher depends on. Exercised without a
//! real watcher: what matters is which filesystem change turns into which
//! catalog operation.

use super::*;

fn included(_: &Path) -> bool {
    false
}

fn excluded(_: &Path) -> bool {
    true
}

#[test]
fn a_new_media_file_is_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clip.mp4");
    std::fs::write(&path, b"x").unwrap();
    assert_eq!(
        effect_for(
            &path,
            &EventKind::Create(notify::event::CreateKind::File),
            false
        ),
        Effect::Index
    );
}

#[test]
fn a_changed_media_file_is_reindexed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("song.mp3");
    std::fs::write(&path, b"x").unwrap();
    assert_eq!(
        effect_for(
            &path,
            &EventKind::Modify(notify::event::ModifyKind::Any),
            false
        ),
        Effect::Index
    );
}

#[test]
fn a_non_media_file_is_never_catalogued() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["notes.txt.bak.tmp", "archive.zip"] {
        let path = dir.path().join(name);
        std::fs::write(&path, b"x").unwrap();
        assert_eq!(
            effect_for(
                &path,
                &EventKind::Create(notify::event::CreateKind::File),
                false
            ),
            Effect::Ignore,
            "{name} is not a catalog kind"
        );
    }
}

#[test]
fn a_dotfile_is_ignored_but_a_nomedia_marker_rescans() {
    let dir = tempfile::tempdir().unwrap();
    let hidden = dir.path().join(".hidden.mp4");
    std::fs::write(&hidden, b"x").unwrap();
    assert_eq!(
        effect_for(
            &hidden,
            &EventKind::Create(notify::event::CreateKind::File),
            false
        ),
        Effect::Ignore
    );
    assert_eq!(
        effect_for(
            &dir.path().join(".nomedia"),
            &EventKind::Create(notify::event::CreateKind::File),
            false
        ),
        Effect::RescanSubtree,
        "a marker changes what a directory contains"
    );
}

#[test]
fn a_removed_file_is_forgotten_and_a_removed_directory_rescans() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("gone.mp4");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(
        effect_for(&file, &EventKind::Remove(RemoveKind::File), false),
        Effect::Forget
    );
    assert_eq!(
        effect_for(
            &dir.path().join("album"),
            &EventKind::Remove(RemoveKind::Folder),
            false
        ),
        Effect::RescanSubtree,
        "a directory boundary moved, so its contents have to be re-read"
    );
}

#[test]
fn an_excluded_path_is_forgotten_rather_than_indexed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cached.mp4");
    std::fs::write(&path, b"x").unwrap();
    for kind in [
        EventKind::Create(notify::event::CreateKind::File),
        EventKind::Modify(notify::event::ModifyKind::Any),
    ] {
        assert_eq!(
            effect_for(&path, &kind, true),
            Effect::Forget,
            "a file the policy excludes must leave the catalog, not enter it"
        );
    }
}

#[test]
fn a_rename_does_not_index_the_old_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.mp4");
    std::fs::write(&path, b"x").unwrap();
    assert_eq!(
        effect_for(
            &path,
            &EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::From)),
            false
        ),
        Effect::Ignore,
        "the create arm is what re-indexes a renamed file"
    );
}
