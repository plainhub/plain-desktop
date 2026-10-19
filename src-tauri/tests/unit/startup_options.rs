use super::*;

#[test]
fn custom_storage_requires_both_roots_without_falling_back_to_personal_storage() {
    assert!(Options::parse(Some("/tmp/data".into()), None, None).is_err());
    assert!(Options::parse(None, Some("/tmp/cache".into()), None).is_err());
    let options =
        Options::parse(Some("/tmp/data".into()), Some("/tmp/cache".into()), None).unwrap();
    let directories = options.directories.unwrap();
    assert_eq!(directories.data, std::path::Path::new("/tmp/data"));
    assert_eq!(directories.cache, std::path::Path::new("/tmp/cache"));
    assert!(!options.private_webview);
}

#[test]
fn webview_privacy_is_explicit_and_rejects_unrecognized_values() {
    let options = Options::parse(None, None, Some("1".into())).unwrap();
    assert!(options.private_webview);
    assert!(options.directories.is_none());
    assert!(
        !Options::parse(None, None, Some("0".into()))
            .unwrap()
            .private_webview
    );
    assert!(Options::parse(None, None, Some("true".into())).is_err());
    assert!(
        Options::parse(None, None, None)
            .unwrap()
            .directories
            .is_none()
    );
}
