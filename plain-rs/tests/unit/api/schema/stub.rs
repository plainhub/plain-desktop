use super::*;

#[test]
fn macos_data_volume_is_not_listed_as_a_second_internal_drive() {
    #[cfg(target_os = "macos")]
    {
        assert!(!should_include_mount("/System/Volumes/Data"));
        assert!(should_include_mount("/"));
        assert!(should_include_mount("/Volumes/External"));
    }

    #[cfg(not(target_os = "macos"))]
    assert!(should_include_mount("/System/Volumes/Data"));
}

#[tokio::test]
async fn mounts_query_returns_system_volumes() {
    let response = crate::api::schema::build_schema()
        .execute("{ mounts { id path mountPoint fsType driveType diskId } }")
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let mounts = response.data.into_json().unwrap();
    assert!(
        mounts["mounts"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
}

#[test]
fn identifies_remote_filesystems_case_insensitively() {
    assert!(is_remote_filesystem("nfs4"));
    assert!(is_remote_filesystem("SMBFS"));
    assert!(is_remote_filesystem("cifs"));
    assert!(!is_remote_filesystem("apfs"));
    assert!(!is_remote_filesystem("ext4"));
}
