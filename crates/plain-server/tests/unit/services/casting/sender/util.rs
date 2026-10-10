//! Unit tests for `src/dlna/util.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn xml_escape_replaces_specials() {
    assert_eq!(
        xml_escape("a<b>c&d\"e'f"),
        "a&lt;b&gt;c&amp;d&quot;e&apos;f"
    );
}

#[test]
fn xml_escape_preserves_normal_text() {
    assert_eq!(xml_escape("hello world"), "hello world");
    assert_eq!(xml_escape(""), "");
}

#[test]
fn xml_escape_replaces_control_chars() {
    // 0x01 should be replaced, tab/lf/cr should pass through.
    let s = "\u{1}foo\tbar\nbaz";
    let escaped = xml_escape(s);
    assert!(!escaped.contains('\u{1}'));
    assert!(escaped.contains("foo\tbar\nbaz"));
}

#[test]
fn truncate_for_log_short_strings_unchanged() {
    assert_eq!(truncate_for_log("hi", 10), "hi");
}

#[test]
fn truncate_for_log_long_strings_truncated() {
    let long = "x".repeat(100);
    let out = truncate_for_log(&long, 10);
    assert!(out.starts_with("xxxxxxxxxx"));
    assert!(out.ends_with("\n...[truncated]"));
}

#[test]
fn truncate_for_log_zero_max_returns_empty() {
    assert_eq!(truncate_for_log("abc", 0), "");
}

#[test]
fn truncate_for_log_does_not_split_utf8() {
    // Each "中" is 3 bytes; truncating at byte 5 would split mid-codepoint.
    // The impl must back up to byte 3 (the boundary after the first "中")
    // rather than panicking or producing invalid UTF-8.
    let s = "中文中文中文";
    let out = truncate_for_log(s, 5);
    assert!(out.starts_with("中"), "got: {out:?}");
    assert!(out.ends_with("\n...[truncated]"));
}
