use super::*;

use std::path::Path;

fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn visited(root: &Path) -> Vec<String> {
    let mut seen: Vec<PathBuf> = Vec::new();
    walk_files(
        root,
        |_| false,
        || false,
        |path| {
            seen.push(path.to_path_buf());
            Ok(())
        },
    )
    .unwrap();
    let mut names: Vec<String> = seen
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn is_cancelled_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::Interrupted)
}

#[test]
fn visits_each_regular_file_exactly_once_in_a_nested_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("top.txt"), "1");
    write_file(&root.join("a/one.txt"), "2");
    write_file(&root.join("a/b/two.txt"), "3");
    write_file(&root.join("a/b/c/three.txt"), "4");

    assert_eq!(
        visited(root),
        ["a/b/c/three.txt", "a/b/two.txt", "a/one.txt", "top.txt"]
    );
}

#[test]
fn a_single_regular_file_may_be_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("only.txt");
    write_file(&file, "1");

    let mut seen: Vec<PathBuf> = Vec::new();
    walk_files(
        &file,
        |_| false,
        || false,
        |path| {
            seen.push(path.to_path_buf());
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(seen, vec![file]);
}

#[test]
fn an_excluded_root_visits_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("a/one.txt"), "1");

    let mut seen: Vec<PathBuf> = Vec::new();
    walk_files(
        root,
        |path| path == root,
        || false,
        |path| {
            seen.push(path.to_path_buf());
            Ok(())
        },
    )
    .unwrap();

    assert!(seen.is_empty());
}

#[test]
fn an_excluded_directory_skips_its_whole_subtree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("keep.txt"), "1");
    write_file(&root.join("drop/one.txt"), "2");
    write_file(&root.join("drop/deep/two.txt"), "3");

    let mut seen: Vec<PathBuf> = Vec::new();
    walk_files(
        root,
        |path| path == root.join("drop"),
        || false,
        |path| {
            seen.push(path.to_path_buf());
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(seen, vec![root.join("keep.txt")]);
}

#[test]
fn a_visit_error_stops_the_walk_and_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("boom.txt"), "1");
    write_file(&root.join("a/one.txt"), "2");
    write_file(&root.join("b/two.txt"), "3");

    let mut seen: Vec<PathBuf> = Vec::new();
    let error = walk_files(
        root,
        |_| false,
        || false,
        |path| {
            seen.push(path.to_path_buf());
            if path.ends_with("boom.txt") {
                return Err(anyhow::anyhow!("visit refused"));
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert!(
        error
            .chain()
            .any(|cause| cause.to_string().contains("visit refused"))
    );
    assert_eq!(seen, vec![root.join("boom.txt")]);
}

#[test]
fn cancelling_before_the_root_stops_every_visit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("one.txt"), "1");

    let mut seen: Vec<PathBuf> = Vec::new();
    let error = walk_files(
        root,
        |_| false,
        || true,
        |path| {
            seen.push(path.to_path_buf());
            Ok(())
        },
    )
    .unwrap_err();

    assert!(is_cancelled_error(&error));
    assert!(seen.is_empty());
}

#[test]
fn cancelling_during_the_walk_stops_every_further_visit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for index in 0..8 {
        write_file(&root.join(format!("file-{index}.txt")), "1");
    }

    let mut seen: Vec<PathBuf> = Vec::new();
    let visits = std::cell::Cell::new(0usize);
    let error = walk_files(
        root,
        |_| false,
        || visits.get() >= 2,
        |path| {
            seen.push(path.to_path_buf());
            visits.set(visits.get() + 1);
            Ok(())
        },
    )
    .unwrap_err();

    assert!(is_cancelled_error(&error));
    assert_eq!(seen.len(), 2);
}

#[test]
fn a_missing_root_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent");

    assert!(walk_files(&missing, |_| false, || false, |_| Ok(())).is_err());
}

#[test]
fn a_relative_root_is_an_error() {
    assert!(walk_files(Path::new("relative/root"), |_| false, || false, |_| Ok(())).is_err());
}

#[cfg(unix)]
#[test]
fn a_symlink_root_is_an_explicit_error() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    fs::create_dir_all(&target).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = walk_files(&link, |_| false, || false, |_| Ok(())).unwrap_err();

    assert!(error.to_string().contains("symlink"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_child_is_never_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let outside = tempfile::tempdir().unwrap();
    write_file(&root.join("real/inside.txt"), "1");
    write_file(&outside.path().join("outside.txt"), "2");
    std::os::unix::fs::symlink(outside.path(), root.join("real/link")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("outside.txt"),
        root.join("real/copy.txt"),
    )
    .unwrap();

    assert_eq!(visited(root), ["real/inside.txt"]);
}

#[cfg(unix)]
#[test]
fn a_fifo_is_skipped_silently() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(&root.join("one.txt"), "1");
    let fifo = root.join("pipe");
    let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::mkfifo(name.as_ptr(), 0o644) },
        0,
        "cannot create the fifo"
    );

    assert_eq!(visited(root), ["one.txt"]);
}

#[cfg(unix)]
#[test]
fn an_excluded_special_root_is_still_invalid() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let mut visits = 0;
    assert!(
        walk_files(
            &path,
            |_| true,
            || false,
            |_| {
                visits += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(visits, 0);
}
