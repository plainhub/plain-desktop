use super::*;

#[test]
fn file_drop_list_preserves_unicode_and_ends_with_two_nulls() {
    let path = Path::new(r"C:\Documents\report 文档 😀.txt");
    let bytes = file_drop(path);
    assert_eq!(
        &bytes[..20],
        &[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0]
    );
    let files: Vec<_> = bytes[20..]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    assert_eq!(&files[files.len() - 2..], &[0, 0]);
    assert_eq!(
        String::from_utf16(&files[..files.len() - 2]).unwrap(),
        path.to_str().unwrap()
    );
}
