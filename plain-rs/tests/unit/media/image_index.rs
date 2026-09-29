//! Unit tests for `src/media/search_index.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use crate::media::scan::MediaFile;

fn mf(path: &str, kind: &str, size: i64, modified: i64) -> MediaFile {
    use std::hash::{Hash, Hasher};
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    MediaFile {
        uuid: format!("{:016x}", h.finish()),
        fsuuid: String::new(),
        ino: 0,
        ctime: 0,
        duration_sec: 0,
        duration_ref_mod: 0,
        duration_ref_size: 0,
        artist: String::new(),
        artist_ref_mod: 0,
        artist_ref_size: 0,
        title: String::new(),
        title_ref_mod: 0,
        title_ref_size: 0,
        path: path.to_string(),
        original_path: path.to_string(),
        name,
        size,
        modified_at: modified,
        r#type: kind.to_string(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    }
}

fn tmp_index() -> MediaSearchIndex {
    let dir = tempfile::tempdir().unwrap();
    let idx = MediaSearchIndex::open(dir.path()).unwrap();
    std::mem::forget(dir);
    idx
}

#[test]
fn search_filter_and_sort() {
    let idx = tmp_index();
    idx.add_media_file(&mf("/mnt/a/Pics/alpha.jpg", "image", 10, 100))
        .unwrap();
    idx.add_media_file(&mf("/mnt/a/Pics/beta.jpg", "image", 20, 300))
        .unwrap();
    idx.add_media_file(&mf("/mnt/b/Clips/gamma.mp4", "video", 30, 200))
        .unwrap();
    idx.commit().unwrap();

    let all = idx
        .search("", Some("image"), None, MediaSort::DateDesc, 0, 10)
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "beta.jpg", "date desc first");

    let size_asc = idx
        .search("", Some("image"), None, MediaSort::SizeAsc, 0, 10)
        .unwrap();
    assert_eq!(size_asc[0].name, "alpha.jpg");

    let count = idx.count("", Some("video"), None).unwrap();
    assert_eq!(count, 1);

    // Fast-field sorts must honor `offset` — pages 2+ used to return the
    // first (limit+offset) docs instead of the [offset, offset+limit) window,
    // silently duplicating early items on every paginated surface.
    let page2 = idx
        .search("", Some("image"), None, MediaSort::DateDesc, 1, 1)
        .unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(
        page2[0].name, "alpha.jpg",
        "second item, not the first again"
    );

    let excl = idx
        .search(
            "excluded_dir:Pics",
            Some("image"),
            None,
            MediaSort::DateDesc,
            0,
            10,
        )
        .unwrap();
    assert!(excl.is_empty(), "excluded dir must filter out images");

    let trash_q = idx.count("trash:true", Some("image"), None).unwrap();
    assert_eq!(trash_q, 0);
}

#[test]
fn excluded_absolute_directory_matches_only_its_tree() {
    let idx = tmp_index();
    for (path, kind) in [
        ("/Users/mac/Projects/smartcoding/app/icons/a.png", "image"),
        ("/Users/mac/Projects/smartcoding/video.mp4", "video"),
        ("/Users/mac/Projects/smartcoding/music.mp3", "audio"),
        ("/Users/mac/Projects/smartcoding-other/keep.png", "image"),
        ("/Users/mac/Pictures/keep.png", "image"),
        ("/Users/mac/My Projects/inside.png", "image"),
    ] {
        idx.add_media_file(&mf(path, kind, 10, 100)).unwrap();
    }
    idx.commit().unwrap();

    let query = "excluded_dir:/Users/mac/Projects/smartcoding/";
    for kind in ["image", "video", "audio"] {
        let results = idx
            .search(query, Some(kind), None, MediaSort::DateDesc, 0, 10)
            .unwrap();
        assert!(
            results
                .iter()
                .all(|item| !item.path.starts_with("/Users/mac/Projects/smartcoding/"))
        );
        assert_eq!(idx.count(query, Some(kind), None).unwrap(), results.len());
    }
    assert_eq!(idx.count(query, Some("image"), None).unwrap(), 3);
    let spaced = idx
        .search(
            "excluded_dir:\"/Users/mac/My Projects/\"",
            Some("image"),
            None,
            MediaSort::DateDesc,
            0,
            10,
        )
        .unwrap();
    assert_eq!(spaced.len(), 3);
    assert!(spaced.iter().all(|item| !item.path.contains("My Projects")));
}

/// An index written by the 2026-09-16 interim layout (all fields incl.
/// `dir`, but `size`/`modified` without the FAST markers) must be
/// detected as stale and recreated — otherwise every sorted query fails
/// with "Field … is not a fast field".
#[test]
fn stale_schema_without_fast_flags_is_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let index_path = dir.path().join(INDEX_DIR_NAME);
    std::fs::create_dir_all(&index_path).unwrap();

    let mut b = Schema::builder();
    let text_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    b.add_text_field("uuid", STRING | STORED);
    b.add_text_field("name", TEXT | STORED);
    b.add_text_field("path", TEXT | STORED);
    b.add_text_field("dir", text_opts);
    b.add_text_field("media_type", STRING | STORED);
    b.add_i64_field("size", STORED | tantivy::schema::INDEXED);
    b.add_i64_field("modified", STORED | tantivy::schema::INDEXED);
    b.add_u64_field("duration", STORED | tantivy::schema::INDEXED);
    b.add_text_field("artist", TEXT | STORED);
    b.add_text_field("title", TEXT | STORED);
    b.add_bool_field("is_trash", STORED | tantivy::schema::INDEXED);
    let old_schema = b.build();

    let old = Index::create_in_dir(&index_path, old_schema.clone()).unwrap();
    {
        let mut writer = old.writer(50_000_000).unwrap();
        let uuid = old_schema.get_field("uuid").unwrap();
        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(uuid, "stale-doc");
        writer.add_document(doc).unwrap();
        writer.commit().unwrap();
    }
    drop(old);

    let idx = MediaSearchIndex::open(dir.path()).unwrap();
    assert_eq!(
        idx.doc_count(),
        0,
        "interim-schema index (no FAST flags) must be recreated empty"
    );
    // And the recreated index answers fast-field sorted queries.
    idx.index_media_file(&mf("/x/a.jpg", "image", 1, 10))
        .unwrap();
    let rows = idx
        .search("", Some("image"), None, MediaSort::DateDesc, 0, 10)
        .unwrap();
    assert_eq!(rows.len(), 1);
}

/// `bucket_top_items` answers per directory with its `n` most recent
/// non-trash files of the type — subdirectory files must not leak into
/// the parent bucket.
#[test]
fn bucket_top_items_most_recent_per_dir() {
    let idx = tmp_index();
    idx.add_media_file(&mf("/pics/old.jpg", "image", 1, 100))
        .unwrap();
    idx.add_media_file(&mf("/pics/new.jpg", "image", 1, 300))
        .unwrap();
    idx.add_media_file(&mf("/pics/mid.jpg", "image", 1, 200))
        .unwrap();
    idx.add_media_file(&mf("/pics/sub/deep.jpg", "image", 1, 500))
        .unwrap();
    idx.add_media_file(&mf("/shots/one.jpg", "image", 1, 50))
        .unwrap();
    idx.add_media_file(&mf("/pics/clip.mp4", "video", 1, 400))
        .unwrap();
    let mut trashed = mf("/pics/trashed.jpg", "image", 1, 999);
    trashed.is_trash = true;
    idx.add_media_file(&trashed).unwrap();
    idx.commit().unwrap();

    let wanted: std::collections::HashSet<String> = [
        "/pics".to_string(),
        "/shots".to_string(),
        "/pics/sub".to_string(),
    ]
    .into_iter()
    .collect();
    let tops = idx.bucket_top_items("image", &wanted, 2).unwrap();

    assert_eq!(
        tops.get("/pics").unwrap(),
        &vec!["/pics/new.jpg".to_string(), "/pics/mid.jpg".to_string()],
        "two most recent, trash excluded, videos ignored, subdir file not leaked"
    );
    assert_eq!(
        tops.get("/shots").unwrap(),
        &vec!["/shots/one.jpg".to_string()]
    );
    assert_eq!(
        tops.get("/pics/sub").unwrap(),
        &vec!["/pics/sub/deep.jpg".to_string()]
    );

    // A dir with no files of the type answers an empty list.
    let none: std::collections::HashSet<String> = ["/empty".to_string()].into_iter().collect();
    let empty = idx.bucket_top_items("image", &none, 4).unwrap();
    assert_eq!(
        empty.get("/empty").map(Vec::as_slice),
        Some(&[] as &[String])
    );
}

#[test]
fn upsert_and_remove() {
    let idx = tmp_index();
    let file = mf("/x/a.jpg", "image", 1, 1);
    idx.index_media_file(&file).unwrap();
    assert_eq!(idx.doc_count(), 1);
    // Same uuid upsert must not duplicate.
    idx.index_media_file(&file).unwrap();
    assert_eq!(idx.doc_count(), 1);
    idx.remove_by_uuid(&file.uuid).unwrap();
    assert_eq!(idx.doc_count(), 0);
}

#[test]
fn name_sort_and_pagination() {
    let idx = tmp_index();
    idx.add_media_file(&mf("/x/c.jpg", "image", 1, 1)).unwrap();
    idx.add_media_file(&mf("/x/a.jpg", "image", 1, 2)).unwrap();
    idx.add_media_file(&mf("/x/b.jpg", "image", 1, 3)).unwrap();
    idx.commit().unwrap();

    let asc: Vec<String> = idx
        .search("", Some("image"), None, MediaSort::NameAsc, 0, 10)
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(asc, ["a.jpg", "b.jpg", "c.jpg"]);

    let desc_page: Vec<String> = idx
        .search("", Some("image"), None, MediaSort::NameDesc, 1, 1)
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(desc_page, ["b.jpg"], "offset 1 limit 1 of [c b a]");
}

#[test]
fn text_and_size_filters() {
    let idx = tmp_index();
    idx.add_media_file(&mf("/x/big cat.jpg", "image", 5_000_000, 1))
        .unwrap();
    idx.add_media_file(&mf("/x/tiny.jpg", "image", 10, 2))
        .unwrap();
    idx.commit().unwrap();

    let hit = idx
        .search("cat", None, None, MediaSort::DateDesc, 0, 10)
        .unwrap();
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].name, "big cat.jpg");

    let big = idx.count("size:>1MB", None, None).unwrap();
    assert_eq!(big, 1);
}

/// `bucket_id:` filters by containing directory — the exact string
/// `mediaBuckets` reports as the bucket id, filesystem root "/" included.
/// (The web media pages filter their lists with `bucket_id:<dir>`; before
/// this arm existed the filter fell through to full-text, where "/" has no
/// tokens, so a non-empty root bucket returned an empty list.)
#[test]
fn bucket_id_filters_by_containing_directory() {
    let idx = tmp_index();
    idx.add_media_file(&mf("/root.jpg", "image", 10, 500))
        .unwrap();
    idx.add_media_file(&mf("/mnt/a/Pics/alpha.jpg", "image", 10, 400))
        .unwrap();
    idx.add_media_file(&mf("/mnt/a/Pics/notes.txt", "doc", 10, 300))
        .unwrap();
    idx.add_media_file(&mf("/mnt/b/Clips/gamma.mp4", "video", 30, 200))
        .unwrap();
    idx.commit().unwrap();

    let paths = |q: &str, kind: Option<&str>| -> Vec<String> {
        idx.search(q, kind, None, MediaSort::DateDesc, 0, 10)
            .unwrap()
            .into_iter()
            .map(|r| r.path)
            .collect()
    };

    // Filesystem-root bucket: only the top-level file.
    assert_eq!(paths("bucket_id:/", None), vec!["/root.jpg".to_string()]);
    // A nested bucket matches exactly its own directory, other kinds included.
    assert_eq!(
        paths("bucket_id:/mnt/a/Pics", None),
        vec![
            "/mnt/a/Pics/alpha.jpg".to_string(),
            "/mnt/a/Pics/notes.txt".to_string()
        ]
    );
    // Composes with the media-kind filter the list resolvers pass.
    assert_eq!(
        paths("bucket_id:/mnt/a/Pics", Some("image")),
        vec!["/mnt/a/Pics/alpha.jpg".to_string()]
    );
    assert_eq!(paths("bucket_id:/mnt/b/Clips", Some("video")).len(), 1);
}

// ----- Docs: kind search, `ext:` filter and the docExtGroups aggregation -----

#[test]
fn doc_search_count_and_ext_filter() {
    let idx = tmp_index();
    idx.add_media_file(&mf("/docs/a/Report.PDF", "doc", 10, 100))
        .unwrap();
    idx.add_media_file(&mf("/docs/b/old.doc", "doc", 20, 200))
        .unwrap();
    let mut trashed = mf("/docs/a/trashed.pdf", "doc", 30, 300);
    trashed.is_trash = true;
    idx.add_media_file(&trashed).unwrap();
    idx.add_media_file(&mf("/docs/a/photo.jpg", "image", 40, 400))
        .unwrap();
    idx.add_media_file(&mf("/docs/a/clip.mp4", "video", 50, 500))
        .unwrap();
    // A doc row whose name has no extension carries no ext term.
    idx.add_media_file(&mf("/docs/a/weird", "doc", 60, 600))
        .unwrap();
    idx.commit().unwrap();

    // Kind-scoped listing/counting ignores other media kinds.
    assert_eq!(idx.count("", Some("doc"), None).unwrap(), 4);
    assert_eq!(
        idx.count("", Some("doc"), Some(false)).unwrap(),
        3,
        "trash:false drops the trashed doc"
    );
    let page = idx
        .search("", Some("doc"), None, MediaSort::DateDesc, 0, 10)
        .unwrap();
    assert_eq!(page.len(), 4);

    // `ext:` is an exact, case-insensitive term match (SQLite-LIKE parity —
    // the sidebar passes back the UPPERCASE group label).
    assert_eq!(idx.count("ext:pdf", Some("doc"), None).unwrap(), 2);
    assert_eq!(idx.count("ext:PDF", Some("doc"), None).unwrap(), 2);
    assert_eq!(idx.count("ext:doc", Some("doc"), None).unwrap(), 1);
    assert_eq!(idx.count("ext:pdf", Some("doc"), Some(false)).unwrap(), 1);
    // Empty ext value carries no clause — it matches all docs, like any
    // other ignored filter (the UI never sends it).
    assert_eq!(idx.count("ext:", Some("doc"), None).unwrap(), 4);

    // NOT semantics flip the ext clause.
    assert_eq!(idx.count("NOT ext:pdf", Some("doc"), None).unwrap(), 2);
}

#[test]
fn doc_ext_groups_excludes_other_kinds_and_survives_upserts() {
    let idx = tmp_index();
    let a = mf("/docs/a/x.pdf", "doc", 10, 100);
    let mut b = mf("/docs/b/y.pdf", "doc", 20, 200);
    let docx = mf("/docs/a/z.docx", "doc", 30, 300);
    idx.add_media_file(&a).unwrap();
    idx.add_media_file(&b).unwrap();
    idx.add_media_file(&docx).unwrap();
    idx.commit().unwrap();

    let groups = idx.doc_ext_groups().unwrap();
    assert_eq!(groups, [("docx".to_string(), 1), ("pdf".to_string(), 2)]);

    // Upsert path: b.pdf moves to .md. The stale segment entry for b.pdf is
    // only a deleted doc — dictionary freqs alone would still report pdf=2;
    // the counting-collector recount must drop it to 1 and add md=1.
    b.name = "y.md".to_string();
    b.r#type = "doc".to_string();
    idx.add_media_file(&b).unwrap();
    idx.commit().unwrap();

    let groups = idx.doc_ext_groups().unwrap();
    assert_eq!(
        groups,
        [
            ("docx".to_string(), 1),
            ("md".to_string(), 1),
            ("pdf".to_string(), 1)
        ]
    );

    // Ext terms only exist on doc rows, so image/video libraries never
    // leak into the groups even though they share the index.
    idx.add_media_file(&mf("/docs/a/pic.jpg", "image", 1, 1))
        .unwrap();
    idx.add_media_file(&mf("/docs/a/song.mp3", "audio", 1, 1))
        .unwrap();
    idx.commit().unwrap();
    let groups = idx.doc_ext_groups().unwrap();
    assert!(groups.iter().all(|(e, _)| e != "jpg" && e != "mp3"));
}

#[test]
fn doc_ext_groups_empty_index_and_no_docs() {
    let idx = tmp_index();
    assert!(idx.doc_ext_groups().unwrap().is_empty());
    idx.add_media_file(&mf("/x/a.jpg", "image", 1, 1)).unwrap();
    idx.commit().unwrap();
    assert!(idx.doc_ext_groups().unwrap().is_empty());
}
