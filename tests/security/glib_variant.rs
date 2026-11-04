use glib::variant::ToVariant;

#[test]
fn strings_survive_the_native_output_pointer_write() {
    let values = ["", "plain", "\u{4e2d}\u{6587}", "last"];
    let variant = values.to_variant();
    let actual: Vec<_> = variant.array_iter_str().unwrap().collect();
    assert_eq!(actual, values);
}

#[test]
fn both_ends_share_the_correct_native_pointer() {
    let variant = ["first", "middle", "last"].to_variant();
    let mut iter = variant.array_iter_str().unwrap();
    assert_eq!(iter.next(), Some("first"));
    assert_eq!(iter.next_back(), Some("last"));
    assert_eq!(iter.next(), Some("middle"));
    assert_eq!(iter.next_back(), None);
    assert_eq!(iter.next(), None);
}

#[test]
fn indexed_and_last_reads_return_native_strings() {
    let variant = ["zero", "one", "two", "three"].to_variant();
    assert_eq!(variant.array_iter_str().unwrap().nth(2), Some("two"));
    assert_eq!(variant.array_iter_str().unwrap().nth_back(1), Some("two"));
    assert_eq!(variant.array_iter_str().unwrap().last(), Some("three"));
}

#[test]
fn empty_arrays_do_not_read_a_native_child() {
    let variant = Vec::<String>::new().to_variant();
    let mut iter = variant.array_iter_str().unwrap();
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    assert_eq!(iter.last(), None);
}
