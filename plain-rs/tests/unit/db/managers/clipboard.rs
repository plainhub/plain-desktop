use super::*;

fn row(id: &str, text: &str, source: &str, label: &str, at: &str) -> ClipboardRow {
    ClipboardRow {
        id: id.into(),
        text: text.into(),
        hash: text.into(),
        source: source.into(),
        label: label.into(),
        sensitive: false,
        created_at: at.into(),
    }
}

#[test]
fn literal_search_count_pagination_and_order() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    for item in [
        row("a", "100%_a", "", "", "2026-10-01T00:00:00Z"),
        row("b", "100xxxa", "peer%_", "", "2026-10-02T00:00:00Z"),
        row("c", "other", "", "100%_a", "2026-10-02T00:00:00Z"),
    ] {
        db.clipboard_save(&item).unwrap();
    }
    assert_eq!(db.clipboard_count("100%_a").unwrap(), 2);
    assert_eq!(db.clipboard_count("peer%_").unwrap(), 1);
    assert_eq!(db.clipboard_page("", 1, 0).unwrap()[0].id, "c");
    assert_eq!(db.clipboard_page("", 1, 1).unwrap()[0].id, "b");
    assert!(db.clipboard_page("", 1, 99).unwrap().is_empty());
    assert_eq!(db.clipboard_page("", 0, -1).unwrap().len(), 1);
    assert_eq!(db.clipboard_count("text:other").unwrap(), 0);
    db.clipboard_save(&row("d", "a\\b' OR 1=1 --", "", "", "2026-10-03T00:00:00Z"))
        .unwrap();
    assert_eq!(db.clipboard_count("a\\b' OR 1=1 --").unwrap(), 1);
}

#[test]
fn destructive_queries_require_explicit_valid_target() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.clipboard_save(&row("a", "100%_a", "peer", "", "2026-10-01T00:00:00Z"))
        .unwrap();
    db.clipboard_save(&row("b", "100xxxa", "peer", "", "2026-10-02T00:00:00Z"))
        .unwrap();
    for invalid in ["", "  ", "source:peer", "all:false", "all:true unknown:bad"] {
        assert!(db.clipboard_delete_query(invalid).is_err(), "{invalid}");
    }
    assert_eq!(db.clipboard_delete_by_ids(&[]).unwrap(), 0);
    assert_eq!(db.clipboard_delete_query("ids:").unwrap(), 0);
    assert_eq!(db.clipboard_delete_query("text:\"100%_a\"").unwrap(), 1);
    assert_eq!(db.clipboard_delete_query("ids:b text:missing").unwrap(), 0);
    assert_eq!(db.clipboard_delete_query("100xxxa").unwrap(), 1);
    assert_eq!(db.clipboard_delete_query("all:true").unwrap(), 0);
}

#[test]
fn record_dedup_is_atomic_and_preserves_origin() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let mut threads = vec![];
    for _ in 0..16 {
        let db = db.clone();
        threads.push(std::thread::spawn(move || {
            crate::library::clipboard::record(&db, "hello 🌍", "peer", "label", true).unwrap()
        }));
    }
    let results = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|(_, inserted)| *inserted).count(), 1);
    assert!(
        results
            .iter()
            .all(|(item, _)| item.id == results[0].0.id && item.sensitive && item.source == "peer")
    );
    assert_eq!(db.clipboard_count_rows().unwrap(), 1);
    let (existing, inserted) =
        crate::library::clipboard::record(&db, "hello 🌍", "", "changed", false).unwrap();
    assert!(!inserted);
    assert_eq!(existing.label, "label");
    assert!(existing.sensitive);
    assert_eq!(
        existing.hash,
        crate::utils::hex::bytes_to_hex(&sha2::Sha256::digest("hello 🌍".as_bytes()))
    );
    db.clipboard_clear().unwrap();
    assert!(
        crate::library::clipboard::record(&db, "hello 🌍", "", "", false)
            .unwrap()
            .1
    );
}

#[test]
fn validation_uses_kotlin_utf16_length() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    for text in ["", " \t\n", "\u{a0}", &"🌍".repeat(128 * 1024 + 1)] {
        assert_eq!(
            crate::library::clipboard::record(&db, text, "", "", false).unwrap_err(),
            "invalid_clipboard_text"
        );
    }
    assert!(
        crate::library::clipboard::record(&db, &"🌍".repeat(128 * 1024), "", "", false)
            .unwrap()
            .1
    );
}

use sha2::Digest;
