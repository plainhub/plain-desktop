//! Unit tests for `src/utils/search_dsl.rs` — ported 1:1 from plain-nas's
//! `tests/unit/search.rs` parser tests (the parser these lock moved here).
use super::*;

#[test]
fn split_in_group_basic() {
    assert_eq!(split_in_group("a b c"), vec!["a", "b", "c"]);
    assert_eq!(
        split_in_group("foo:'bar baz' x"),
        vec!["foo:'bar baz'", "x"]
    );
    assert_eq!(
        split_in_group("foo:\"bar\" baz"),
        vec!["foo:\"bar\"", "baz"]
    );
    assert_eq!(split_in_group("a\\ b c"), vec!["a b", "c"]);
}

#[test]
fn remove_quotation_strips_both_kinds() {
    assert_eq!(remove_quotation("'abc'"), "abc");
    assert_eq!(remove_quotation("\"abc\""), "abc");
    assert_eq!(remove_quotation("abc"), "abc");
    assert_eq!(remove_quotation("'a b'"), "a b");
}

#[test]
fn detect_group_type_prefers_longest_op() {
    assert_eq!(detect_group_type("<=100MB"), "<=");
    assert_eq!(detect_group_type(">=1GB"), ">=");
    assert_eq!(detect_group_type("!=0"), "!=");
    assert_eq!(detect_group_type("=42"), "=");
    assert_eq!(detect_group_type(">10"), ">");
    assert_eq!(detect_group_type("<5"), "<");
    assert_eq!(detect_group_type("100"), "=");
}

#[test]
fn parse_simple_text() {
    let f = parse("hello world");
    assert_eq!(f.len(), 2);
    assert_eq!(
        f[0],
        FilterField {
            name: "text".into(),
            op: "".into(),
            value: "hello".into()
        }
    );
    assert_eq!(
        f[1],
        FilterField {
            name: "text".into(),
            op: "".into(),
            value: "world".into()
        }
    );
}

#[test]
fn parse_field_with_op() {
    let f = parse("size:>1024");
    assert_eq!(
        f,
        vec![FilterField {
            name: "size".into(),
            op: ">".into(),
            value: "1024".into()
        }]
    );
}

#[test]
fn parse_is_dir() {
    let f = parse("is:dir");
    assert_eq!(
        f,
        vec![FilterField {
            name: "dir".into(),
            op: "".into(),
            value: "true".into()
        }]
    );
}

#[test]
fn parse_not_inverts_next() {
    let f = parse("NOT size:>10");
    // NOT flips > to <=
    assert_eq!(
        f,
        vec![FilterField {
            name: "size".into(),
            op: "<=".into(),
            value: "10".into()
        }]
    );
}

#[test]
fn parse_not_inverts_eq() {
    let f = parse("NOT kind:=dir");
    assert_eq!(
        f,
        vec![FilterField {
            name: "kind".into(),
            op: "!=".into(),
            value: "dir".into()
        }]
    );
}
