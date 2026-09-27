//! Unit tests for `src/log.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Performance contract: a disabled level must not pay ANY formatting
/// cost. The log macros gate on `enabled(level)` before rendering, so
/// field/format expressions never evaluate when the level is off —
/// observable here by a side-effecting field expression that must stay
/// unevaluated at a disabled level (locked: `cargo test disabled_level`).
#[test]
fn disabled_level_skips_formatting_entirely() {
    static FMT_EVALS: AtomicUsize = AtomicUsize::new(0);
    let saved = level();

    set_level(Level::Error);
    crate::log::debug!(
        evals = {
            FMT_EVALS.fetch_add(1, Ordering::SeqCst);
            1
        },
        "debug must not format while Error-level is active"
    );
    assert_eq!(
        FMT_EVALS.load(Ordering::SeqCst),
        0,
        "debug! at a disabled level must not evaluate field expressions"
    );

    set_level(Level::Debug);
    crate::log::debug!(
        evals = {
            FMT_EVALS.fetch_add(1, Ordering::SeqCst);
            1
        },
        "debug formats while enabled"
    );
    assert_eq!(FMT_EVALS.load(Ordering::SeqCst), 1);

    set_level(saved);
}

/// Severity contract: a threshold shows itself and everything MORE
/// severe, suppresses everything more verbose. Warn/Error must never
/// be swallowed by a less-severe threshold — the old inverted
/// comparison did exactly that at the default `info` level.
#[test]
fn enabled_follows_severity_ordering() {
    let saved = level();
    let all = [
        Level::Trace,
        Level::Debug,
        Level::Info,
        Level::Warn,
        Level::Error,
    ];
    for threshold in all {
        set_level(threshold);
        assert_eq!(level(), threshold);
        for l in all {
            assert_eq!(
                enabled(l),
                (l as usize) >= (threshold as usize),
                "threshold {threshold:?}, level {l:?}"
            );
        }
    }
    set_level(saved);
}

// ---------------------------------------------------------------------------
// On-disk sink + newest-first reader (developer UI `appLogs` surface)
// ---------------------------------------------------------------------------

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;

/// The sink is a process-global; the three sink tests must not interleave
/// (same pattern as the shared tag-db tests: lock, act, detach).
static SINK_SEQ: StdMutex<()> = StdMutex::new(());

// Nanos-unique temp path so repeated runs never reuse a stale file.
fn tmp_log(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("plain-nas-log-{tag}-{nanos}.log"))
}

fn write_lines(path: &std::path::Path, lines: &[&str]) {
    let mut f = std::fs::File::create(path).unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

/// The file sink records every ENABLED record with a UTC timestamp
/// prefix; a disabled level produces no file bytes. Assertions match
/// marker lines (not total line counts) so unrelated concurrent logging
/// cannot flake the test.
#[test]
fn file_sink_writes_enabled_levels_only() {
    let _seq = SINK_SEQ.lock().unwrap();
    let path = tmp_log("sink");
    set_file(&path);
    let saved = level();
    set_level(Level::Info);

    crate::log::info!("sinkmarker-info {}", 2);
    crate::log::debug!("sinkmarker-debug must not land");
    crate::log::warn!("sinkmarker-warn");

    set_level(saved);
    *SINK.lock().unwrap() = None;

    let content = std::fs::read_to_string(&path).unwrap();
    let info_line = content
        .lines()
        .find(|l| l.ends_with(" INFO sinkmarker-info 2"))
        .expect("info line in file");
    assert!(content.contains(" WARN sinkmarker-warn"), "{content:?}");
    assert!(
        !content.contains("sinkmarker-debug"),
        "disabled level must not write: {content:?}"
    );
    // UTC `YYYY-MM-DD HH:MM:SS.mmm` prefix — 23 chars before the level.
    let ts = &info_line[..info_line.len() - " INFO sinkmarker-info 2".len()];
    assert_eq!(ts.len(), 23, "timestamp {ts:?}");
    assert_eq!(ts.as_bytes()[4], b'-');
    assert_eq!(ts.as_bytes()[10], b' ');
    assert_eq!(ts.as_bytes()[13], b':');
    assert_eq!(ts.as_bytes()[19], b'.');
    std::fs::remove_file(&path).ok();
}

/// newest-first contract: returns at most `limit` lines starting at the
/// `offset`-th newest line, works across 64 KB block boundaries, skips
/// empty lines, trims `\r`, survives a missing trailing newline.
#[test]
fn read_lines_newest_first_matches_plain_app_semantics() {
    let path = tmp_log("read");

    // Missing / empty file → empty result.
    assert!(read_lines_newest_first(&path, None, 0, 10).is_empty());
    std::fs::write(&path, b"").unwrap();
    assert!(read_lines_newest_first(&path, None, 0, 10).is_empty());

    // 3000 short lines ≈ several 64 KB blocks. Newest = highest number.
    let mut f = std::fs::File::create(&path).unwrap();
    for i in 0..3000 {
        writeln!(f, "line-{i}").unwrap();
    }
    drop(f);

    let first_page = read_lines_newest_first(&path, None, 0, 200);
    assert_eq!(first_page.len(), 200);
    assert_eq!(first_page[0], "line-2999", "newest line first");
    assert_eq!(first_page[199], "line-2800");

    let second_page = read_lines_newest_first(&path, None, 200, 200);
    assert_eq!(
        second_page[0], "line-2799",
        "offset windows into older lines"
    );

    let tail = read_lines_newest_first(&path, None, 2990, 100);
    assert_eq!(tail.len(), 10, "short read near the oldest line");
    assert_eq!(tail[9], "line-0");

    assert!(
        read_lines_newest_first(&path, None, 0, 0).is_empty(),
        "limit 0"
    );

    // CRLF + missing trailing newline + blank line skipped.
    let path2 = tmp_log("read2");
    std::fs::write(&path2, b"old\r\n\nnewest no newline").unwrap();
    assert_eq!(
        read_lines_newest_first(&path2, None, 0, 10),
        vec!["newest no newline", "old"]
    );
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&path2).ok();
}

/// `clear_file` truncates; the sink keeps appending afterwards at the
/// new (empty) end. Standalone truncate works for non-sink paths too.
#[test]
fn clear_file_truncates_and_sink_recovers() {
    let _seq = SINK_SEQ.lock().unwrap();
    let path = tmp_log("clear");
    write_lines(&path, &["old-1", "old-2"]);
    set_file(&path);
    let saved = level();
    set_level(Level::Info);
    crate::log::info!("clearmarker-kept");
    clear_file(&path);
    crate::log::info!("clearmarker-after-clear");
    set_level(saved);
    *SINK.lock().unwrap() = None;

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(!content.contains("old-1"), "truncated: {content:?}");
    assert!(
        !content.contains("clearmarker-kept"),
        "pre-clear line gone: {content:?}"
    );
    assert!(
        content.contains(" INFO clearmarker-after-clear\n"),
        "{content:?}"
    );

    // Truncating a non-sink path creates/truncates standalone.
    let other = tmp_log("clear-other");
    write_lines(&other, &["x"]);
    clear_file(&other);
    assert_eq!(std::fs::metadata(&other).unwrap().len(), 0);
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&other).ok();
}

/// Startup hands the sink a path whose `logs/` parent may not exist yet —
/// `set_file` must create it instead of silently disabling the sink.
#[test]
fn set_file_creates_missing_parent_dirs() {
    let _seq = SINK_SEQ.lock().unwrap();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("plain-nas-log-mkdir-{nanos}"));
    let path = root.join("logs").join("latest.log");

    set_file(&path);
    let saved = level();
    set_level(Level::Info);
    crate::log::info!("mkdir-marker");
    set_level(saved);
    *SINK.lock().unwrap() = None;

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains(" INFO mkdir-marker"), "{content:?}");
    std::fs::remove_dir_all(&root).ok();
}
