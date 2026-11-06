use super::*;

#[test]
fn file_drop_list_preserves_unicode_and_ends_with_two_nulls() {
    let path = Path::new(r"C:\Documents\report 文档 😀.txt");
    let files = file_list(path);
    assert_eq!(&files[files.len() - 2..], &[0, 0]);
    assert_eq!(
        String::from_utf16(&files[..files.len() - 2]).unwrap(),
        path.to_str().unwrap()
    );
    assert_eq!(std::mem::size_of::<DropFiles>(), 20);
}
