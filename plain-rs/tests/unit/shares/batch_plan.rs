use super::*;
fn file(path: &str, name: &str, dir: bool, size: i64) -> File {
    File {
        virtual_path: path.into(),
        name: name.into(),
        is_dir: dir,
        size,
        mime_type: String::new(),
        has_thumb: false,
    }
}
#[test]
fn sync_paths_preserve_tree_without_duplicating_file_basename() {
    let mut walker = Walker::new(
        Kind::Sync,
        vec![file("r", "root", true, 0)],
        "/chosen/",
        "/downloads",
    )
    .unwrap();
    assert_eq!(walker.next_directory().unwrap().as_deref(), Some("r"));
    walker
        .supply(vec![
            file("r/b", "b.txt", false, 3),
            file("r/d", "子目录", true, 0),
        ])
        .unwrap();
    assert_eq!(walker.next_directory().unwrap().as_deref(), Some("r/d"));
    walker
        .supply(vec![file("r/d/f", "a +?#%.txt", false, 5)])
        .unwrap();
    assert!(walker.next_directory().unwrap().is_none());
    let plan = walker.finish().unwrap();
    assert_eq!((plan.total_files, plan.total_size), (2, 8));
    assert_eq!(plan.targets[0].write_dir, "/chosen/root/子目录");
    assert_eq!(plan.targets[0].entry_name, "root/子目录/a +?#%.txt");
    assert_eq!(plan.targets[1].write_dir, "/chosen/root");
    assert!(plan.targets.iter().all(|target| !target.store_to_downloads));
}
#[test]
fn public_downloads_are_flat_only_for_selected_files_and_zip_names_keep_hierarchy() {
    for kind in [Kind::Multi, Kind::Zip] {
        let mut walker = Walker::new(
            kind,
            vec![file("one", "one.txt", false, 1), file("r", "root", true, 0)],
            "",
            "/downloads/PlainApp",
        )
        .unwrap();
        assert_eq!(walker.next_directory().unwrap().as_deref(), Some("r"));
        walker
            .supply(vec![file("r/two", "two.txt", false, 2)])
            .unwrap();
        assert!(walker.next_directory().unwrap().is_none());
        let plan = walker.finish().unwrap();
        assert_eq!(plan.targets[0].store_to_downloads, kind == Kind::Multi);
        assert!(!plan.targets[1].store_to_downloads);
        assert_eq!(plan.targets[1].entry_name, "root/two.txt");
        assert_eq!(
            plan.targets[1].write_dir,
            if kind == Kind::Multi {
                "/downloads/PlainApp/root"
            } else {
                ""
            }
        );
    }
    let mut walker = Walker::new(
        Kind::File,
        vec![file("f", "f", false, 0)],
        "/",
        "/downloads",
    )
    .unwrap();
    assert!(walker.next_directory().unwrap().is_none());
    assert_eq!(walker.finish().unwrap().targets[0].write_dir, "/");
}
#[test]
fn cycles_traversal_duplicate_destinations_negative_sizes_and_overflow_fail_closed() {
    for bad in ["../escape", ".", "..", "", "a/b", "a\\b", "nul\0x"] {
        let mut walker = Walker::new(
            Kind::File,
            vec![file("f", bad, false, 0)],
            "/out",
            "/downloads",
        )
        .unwrap();
        assert!(walker.next_directory().is_err());
    }
    let mut walker = Walker::new(
        Kind::Sync,
        vec![file("r", "root", true, 0)],
        "/out",
        "/downloads",
    )
    .unwrap();
    walker.next_directory().unwrap();
    walker.supply(vec![file("r", "again", true, 0)]).unwrap();
    assert!(walker.next_directory().is_err());
    for entries in [
        vec![file("a", "same", false, 1), file("b", "same", false, 2)],
        vec![file("a", "a", false, -1)],
        vec![file("a", "a", false, i64::MAX), file("b", "b", false, 1)],
    ] {
        let mut walker = Walker::new(Kind::Multi, entries, "/out", "/downloads").unwrap();
        assert!(walker.next_directory().is_err());
    }
}
#[test]
fn overlapping_selection_is_deduplicated_and_empty_directories_are_valid() {
    let mut walker = Walker::new(
        Kind::Multi,
        vec![file("r", "root", true, 0), file("r/f", "f", false, 10)],
        "",
        "/downloads",
    )
    .unwrap();
    walker.next_directory().unwrap();
    walker.supply(vec![file("r/f", "f", false, 10)]).unwrap();
    assert!(walker.next_directory().unwrap().is_none());
    assert_eq!(walker.finish().unwrap().total_files, 1);
    let mut walker = Walker::new(
        Kind::Zip,
        vec![file("r", "empty", true, 0)],
        "",
        "/downloads",
    )
    .unwrap();
    walker.next_directory().unwrap();
    walker.supply(vec![]).unwrap();
    walker.next_directory().unwrap();
    assert_eq!(walker.finish().unwrap().total_size, 0);
}

#[test]
fn directory_depth_and_file_count_are_bounded() {
    let mut walker = Walker::new(
        Kind::Sync,
        vec![file("0", "root", true, 0)],
        "/out",
        "/downloads",
    )
    .unwrap();
    for index in 1..=64 {
        walker.next_directory().unwrap();
        walker
            .supply(vec![file(&index.to_string(), "child", true, 0)])
            .unwrap();
    }
    assert!(walker.next_directory().is_err());
    let mut walker = Walker::new(
        Kind::Sync,
        vec![file("r", "root", true, 0)],
        "/out",
        "/downloads",
    )
    .unwrap();
    walker.next_directory().unwrap();
    assert!(
        walker
            .supply(vec![file("f", "f", false, 0); 100_000])
            .is_err()
    );
}
