//! Unit tests for `src/db/events.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
static SEQ: AtomicUsize = AtomicUsize::new(0);

/// Isolated Db for this test (same pattern as tests/unit/db/tags.rs).
fn fresh() -> EventLog<'static> {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let p = env::temp_dir().join(format!("plain-nas-events-{n}"));
    let _ = std::fs::remove_dir_all(&p);
    let db = Box::leak(Box::new(Db::open(&p).unwrap()));
    EventLog::new(db)
}

/// Seed three events with monotonically distinct created_at stamps
/// (clocks with coarse resolution need the sleeps to stay deterministic).
fn seed(log: &EventLog) {
    log.add("login", "alice", "c1").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    log.add("logout", "Bob", "c2").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    log.add("format_disk", "/dev/sda", "c3").unwrap();
}

#[test]
fn list_returns_newest_first_with_offset_and_limit() {
    let log = fresh();
    seed(&log);
    let all = log.list(0, 10, None).unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].r#type, "format_disk", "newest first");
    assert_eq!(all[2].r#type, "login", "oldest last");

    let page = log.list(1, 1, None).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].r#type, "logout", "offset=1 limit=1 → middle row");

    assert_eq!(log.list(5, 10, None).unwrap().len(), 0, "offset past end");
}

#[test]
fn list_filters_by_case_insensitive_needle_before_paging() {
    let log = fresh();
    seed(&log);
    // Needle hits type ("logout") — mixed case on purpose.
    let hits = log.list(0, 10, Some("LOGO")).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].r#type, "logout");
    // Needle hits message, not the type.
    let hits = log.list(0, 10, Some("/dev/sda")).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].r#type, "format_disk");
    // Filter applies before offset/limit: "a" matches rows 3 (format_disk,
    // newest) and 1 (login/alice), so offset=1 over the filtered, newest-
    // first subset yields the login row only.
    let paged = log.list(1, 10, Some("a")).unwrap();
    assert_eq!(paged.len(), 1);
    assert_eq!(paged[0].r#type, "login");
}
