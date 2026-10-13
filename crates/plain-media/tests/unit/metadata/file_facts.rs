//! Unit tests for `src/metadata/file_facts.rs` — moved out-of-line;
//! compiled as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use plain_rs::db::MediaFileMetadata;
use plain_rs::enums::DataType;
use plain_rs::utils::dbtime::iso_from_unix_millis;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../plain-rs/testdata"
    ))
    .join(name)
}

/// The row format is the fixed-width `...Z` text the catalog sorts on, so a
/// rendered timestamp must equal `dbtime`'s own rendering of the same instant.
fn dbtime_of(at: SystemTime) -> String {
    let millis = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    iso_from_unix_millis(millis)
}

/// Directory contents with size and mtime, so a side effect anywhere in the
/// tree (cache file, cover write, temp file) shows up as a difference.
fn listing(dir: &Path) -> Vec<(String, u64, SystemTime)> {
    let mut out: Vec<(String, u64, SystemTime)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let meta = entry.metadata().unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                meta.len(),
                meta.modified().unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// The four kinds the media index covers
// ---------------------------------------------------------------------------

#[test]
fn accepted_kinds_are_exactly_the_media_index_wire_values() {
    let cases = [
        (DataType::Audio, "audio", "audio-tagged.mp3"),
        (DataType::Video, "video", "video-h265.mp4"),
        (DataType::Image, "image", "v-prog420.jpg"),
        (DataType::Doc, "doc", "html_to_markdown.md"),
    ];
    for (data_type, kind, name) in cases {
        assert_eq!(data_type.media_type_str(), Some(kind));
        let m = read_file_metadata(&fixture(name), data_type).unwrap();
        assert_eq!(m.data_type, data_type);
        assert_eq!(
            m.path,
            std::fs::canonicalize(fixture(name))
                .unwrap()
                .to_str()
                .unwrap()
        );
    }
}

#[test]
fn data_types_outside_the_media_index_are_errors_even_for_a_real_file() {
    let path = fixture("v-photo-prog.jpg");
    for data_type in [
        DataType::Default,
        DataType::Sms,
        DataType::Contact,
        DataType::Note,
        DataType::FeedEntry,
        DataType::Call,
        DataType::Package,
        DataType::File,
        DataType::AppFile,
    ] {
        assert!(
            read_file_metadata(&path, data_type).is_err(),
            "{data_type:?} has no media kind"
        );
    }
}

// ---------------------------------------------------------------------------
// Real OS facts: path, bucket, size, timestamps
// ---------------------------------------------------------------------------

#[test]
fn audio_row_carries_probe_tags_and_real_stat_facts() {
    let path = fixture("audio-tagged.mp3");
    let canonical = std::fs::canonicalize(&path).unwrap();
    let meta = std::fs::metadata(&path).unwrap();

    let m = read_file_metadata(&path, DataType::Audio).unwrap();

    assert_eq!(m.data_type, DataType::Audio);
    assert_eq!(m.path, canonical.to_str().unwrap());
    assert_eq!(m.bucket_id, canonical.parent().unwrap().to_str().unwrap());
    assert_eq!(m.size_bytes, i64::try_from(meta.len()).unwrap());
    assert_eq!(m.artist, "Hydrate Artist");
    assert_eq!(m.title, "Hydrate Title");
    assert_eq!(m.album_file_id, "");
    let probed = crate::metadata::probe_media(m.path.as_str(), "audio");
    assert_eq!(m.duration_ms, i64::from(probed.duration_secs) * 1000);
    assert!(m.duration_ms > 0);
    assert_eq!((m.width, m.height, m.rotation), (0, 0, 0));
    assert_eq!(m.taken_at, None);
}

#[test]
fn timestamps_use_the_dbtime_row_format() {
    let path = fixture("v-prog-gray.jpg");
    let meta = std::fs::metadata(&path).unwrap();
    let modified = meta.modified().unwrap();

    let m = read_file_metadata(&path, DataType::Image).unwrap();

    assert_eq!(m.updated_at, dbtime_of(modified));
    assert!(m.updated_at.ends_with('Z'), "{}", m.updated_at);
    assert!(m.created_at.ends_with('Z'), "{}", m.created_at);
    let born = birth_unix_secs(&meta)
        .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64));
    assert_eq!(m.created_at, dbtime_of(born.unwrap_or(modified)));
}

#[test]
fn created_at_falls_back_to_modified_when_the_os_records_no_birth_time() {
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let born = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);

    assert_eq!(created_at_of(None, modified), "2023-11-14T22:13:20Z");
    assert_eq!(created_at_of(Some(born), modified), "2020-09-13T12:26:40Z");
    assert_eq!(
        row_timestamp(SystemTime::UNIX_EPOCH),
        "1970-01-01T00:00:00Z"
    );
}

#[test]
fn timestamps_sort_chronologically_as_text() {
    let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let later = base + Duration::from_secs(1);

    assert!(row_timestamp(base) < row_timestamp(later));
}

// ---------------------------------------------------------------------------
// Probe results and the title fallback
// ---------------------------------------------------------------------------

#[test]
fn video_row_duration_comes_from_the_container_probe() {
    let m = read_file_metadata(&fixture("video-h264-baseline.mp4"), DataType::Video).unwrap();

    assert_eq!(m.duration_ms, 6_000);
    assert!(m.artist.is_empty());
    assert_eq!(m.title, "video-h264-baseline");
    assert_eq!((m.width, m.height, m.rotation), (0, 0, 0));
    assert_eq!(m.taken_at, None);
}

#[test]
fn tagless_audio_falls_back_to_the_file_stem() {
    let m = read_file_metadata(&fixture("audio-untagged.mp3"), DataType::Audio).unwrap();

    assert_eq!(m.title, "audio-untagged");
    assert!(m.artist.is_empty());
    assert!(m.duration_ms > 0);
}

#[test]
fn doc_row_keeps_real_stat_facts_and_unknown_optional_metadata() {
    let m = read_file_metadata(&fixture("public-schema.graphqls"), DataType::Doc).unwrap();

    assert_eq!(m.data_type, DataType::Doc);
    assert_eq!(m.title, "public-schema");
    assert!(m.artist.is_empty());
    assert_eq!(m.duration_ms, 0);
    assert_eq!((m.width, m.height, m.rotation), (0, 0, 0));
    assert_eq!(m.taken_at, None);
    assert!(m.size_bytes > 0);
    assert!(!m.created_at.is_empty() && !m.updated_at.is_empty());
}

// ---------------------------------------------------------------------------
// Image dimensions, and formats the enabled image features cannot decode
// ---------------------------------------------------------------------------

#[test]
fn image_row_reads_pixel_dimensions_from_the_image_crate() {
    let big = read_file_metadata(&fixture("v-photo-prog.jpg"), DataType::Image).unwrap();
    assert_eq!((big.width, big.height), (1200, 800));
    assert_eq!(big.rotation, 0, "fixture carries no EXIF orientation");
    assert_eq!(big.taken_at, None);
    assert_eq!(big.title, "v-photo-prog");
    assert_eq!(big.duration_ms, 0);

    let small = read_file_metadata(&fixture("v-prog-gray.jpg"), DataType::Image).unwrap();
    assert_eq!((small.width, small.height), (160, 120));
}

#[test]
fn formats_the_image_features_cannot_decode_still_produce_a_row() {
    // HEIC/AVIF/BMP are valid files the enabled `image` features cannot
    // read: they must reach the catalog with unknown dimensions rather than
    // fail, or a scan would hide them.
    let dir = tempfile::tempdir().unwrap();
    for name in ["photo.heic", "photo.avif", "photo.bmp", "photo.tiff"] {
        let path = dir.path().join(name);
        std::fs::copy(fixture("v-photo-prog.jpg"), &path).unwrap();

        let m = read_file_metadata(&path, DataType::Image).unwrap();

        assert_eq!((m.width, m.height, m.rotation), (0, 0, 0), "{name}");
        assert_eq!(m.taken_at, None, "{name}");
        assert_eq!(m.title, "photo", "{name}");
        assert!(m.size_bytes > 0, "{name}");
        assert!(
            !m.created_at.is_empty() && !m.updated_at.is_empty(),
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------
// Errors, never fabricated rows
// ---------------------------------------------------------------------------

#[test]
fn missing_path_and_directory_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        read_file_metadata(&dir.path().join("nope.jpg"), DataType::Image).is_err(),
        "missing file"
    );
    assert!(
        read_file_metadata(dir.path(), DataType::Image).is_err(),
        "directory is not a media file"
    );
    let file = dir.path().join("x.jpg");
    std::fs::write(&file, b"not an image").unwrap();
    assert!(
        read_file_metadata(&file, DataType::Image).is_ok(),
        "an undecodable image is still a real file with a row"
    );
}

#[cfg(unix)]
#[test]
fn paths_that_are_not_utf8_are_errors() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(OsStr::from_bytes(b"bad-\xFF-name.jpg"));
    let error = read_file_metadata(&path, DataType::Image).unwrap_err();
    assert!(error.to_string().contains("not valid UTF-8"));
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_is_an_error() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    if dir.path().metadata().unwrap().uid() == 0 {
        return;
    }
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    let file = locked.join("a.jpg");
    std::fs::write(&file, b"x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = read_file_metadata(&file, DataType::Image);

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Read-only by construction
// ---------------------------------------------------------------------------

#[test]
fn reading_facts_writes_nothing_and_repeats_identically() {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("audio-tagged.mp3", DataType::Audio),
        ("video-h265.mp4", DataType::Video),
        ("v-prog-gray.jpg", DataType::Image),
        ("html_to_markdown.md", DataType::Doc),
    ];
    for (name, _) in files {
        std::fs::copy(fixture(name), dir.path().join(name)).unwrap();
    }
    let before = listing(dir.path());

    let read_all = || {
        files
            .iter()
            .map(|(name, data_type)| {
                read_file_metadata(&dir.path().join(*name), *data_type).unwrap()
            })
            .collect::<Vec<MediaFileMetadata>>()
    };
    let first = read_all();
    let second = read_all();

    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    assert_eq!(
        before,
        listing(dir.path()),
        "no file was created or touched"
    );
}

#[test]
fn probe_availability_keeps_unknown_distinct_from_valid_untagged_audio() {
    let directory = tempfile::tempdir().unwrap();
    let damaged = directory.path().join("damaged.mp3");
    std::fs::write(&damaged, b"unrecognized audio bytes").unwrap();
    let unknown = read_file_metadata_with_availability(&damaged, DataType::Audio).unwrap();
    assert!(!unknown.available.tags);
    assert!(!unknown.available.duration);
    assert!(!unknown.available.album_file_id);
    let untagged =
        read_file_metadata_with_availability(&fixture("audio-untagged.mp3"), DataType::Audio)
            .unwrap();
    assert!(untagged.available.tags);
    assert!(untagged.available.duration);
    assert!(untagged.metadata.artist.is_empty());
}

#[test]
fn unsupported_image_metadata_stays_visible_without_reporting_unknown_fields_as_available() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unknown.heic");
    std::fs::write(&path, b"unsupported image header").unwrap();
    let facts = read_file_metadata_with_availability(&path, DataType::Image).unwrap();
    assert_eq!(facts.metadata.data_type, DataType::Image);
    assert!(facts.available.tags);
    assert!(!facts.available.dimensions);
    assert!(!facts.available.rotation);
    assert!(!facts.available.taken_at);
}
