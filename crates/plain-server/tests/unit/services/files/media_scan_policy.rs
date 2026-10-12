use super::*;

#[test]
fn loads_real_cache_and_config_exclusions_without_global_path_state() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    let excluded = directory.path().join("excluded");
    let config = directory.path().join("config.toml");
    std::fs::write(
        &config,
        format!("[media_scan]\nexcluded_dirs = {}\n", excluded.display()),
    )
    .unwrap();
    let policy = Policy::load(&cache, Some(&config)).unwrap();
    assert_eq!(policy.cache_directory, cache);
    assert_eq!(policy.excluded_roots, vec![excluded.to_str().unwrap()]);
    assert!(
        Policy::load(&cache, None)
            .unwrap()
            .excluded_roots
            .is_empty()
    );
    assert!(
        Policy::load(&cache, Some(&directory.path().join("missing.toml")))
            .unwrap()
            .excluded_roots
            .is_empty()
    );
}

#[test]
fn refuses_relative_paths_and_configuration_read_failures() {
    let directory = tempfile::tempdir().unwrap();
    assert!(Policy::load(Path::new("relative"), None).is_err());
    assert!(Policy::load(directory.path(), Some(Path::new("relative.toml"))).is_err());
    assert!(Policy::load(directory.path(), Some(directory.path())).is_err());
    let config = directory.path().join("config.toml");
    std::fs::write(&config, "[media_scan]\nexcluded_dirs = relative\n").unwrap();
    assert!(Policy::load(directory.path(), Some(&config)).is_err());
}
