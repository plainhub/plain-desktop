use super::*;
use std::io::Write;

#[test]
fn archive_entry_bytes_are_exact_and_repeated_reads_do_not_replace_the_cache() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("files.zip");
    let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
    zip.start_file("nested/note.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"actual entry bytes").unwrap();
    zip.finish().unwrap();
    let reference = format!("{}!zip!/nested/note.txt", archive.display());
    let cache = directory.path().join("cache");
    let path = extract(&cache, &reference).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"actual entry bytes");
    #[cfg(unix)]
    let before = {
        use std::os::unix::fs::MetadataExt;
        path.metadata().unwrap().ino()
    };
    assert_eq!(extract(&cache, &reference).unwrap(), path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(path.metadata().unwrap().ino(), before);
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"actual entry bytes");
    assert!(extract(&cache, &format!("{}!zip!/missing.txt", archive.display())).is_err());
    assert_eq!(std::fs::read_dir(cache).unwrap().count(), 1);
}
