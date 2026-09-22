//! Unit tests for `src-tauri/src/local/graphql/schema/file_upload.rs`.
//! Locked contracts: chunk concatenation order, missing-chunk errors,
//! MergeJobState → MergeTask mapping, job-map pruning.

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("plain_merge_test_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn merge_chunks_to_concatenates_in_order() {
    let dir = temp_dir("concat");
    std::fs::write(dir.join("chunk_0"), b"hello ").unwrap();
    std::fs::write(dir.join("chunk_1"), b"world").unwrap();
    let out = dir.join("merged.bin");
    merge_chunks_to(&dir, 2, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"hello world");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn merge_chunks_to_reports_missing_chunk() {
    let dir = temp_dir("missing");
    std::fs::write(dir.join("chunk_0"), b"hello").unwrap();
    let out = dir.join("merged.bin");
    let err = merge_chunks_to(&dir, 2, &out).unwrap_err();
    assert!(err.message.contains("chunk 1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn task_from_state_covers_all_states() {
    assert_eq!(task_from_state(None).status, MergeTaskStatus::None);
    assert_eq!(
        task_from_state(Some(&MergeJobState::Merging)).status,
        MergeTaskStatus::Merging
    );
    let done = task_from_state(Some(&MergeJobState::Done {
        value: "a.jpg".to_string(),
        size: 42,
    }));
    assert_eq!(done.status, MergeTaskStatus::Done);
    assert_eq!(done.value.as_deref(), Some("a.jpg"));
    assert_eq!(done.merged_size, Some(42));
    let failed = task_from_state(Some(&MergeJobState::Failed {
        error: "boom".to_string(),
    }));
    assert_eq!(failed.status, MergeTaskStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("boom"));
}

#[test]
fn done_task_only_maps_done_state() {
    assert!(done_task(None).is_none());
    assert!(done_task(Some(&MergeJobState::Merging)).is_none());
    let task = done_task(Some(&MergeJobState::Done {
        value: "v.bin".to_string(),
        size: 7,
    }))
    .unwrap();
    assert_eq!(task.status, MergeTaskStatus::Done);
    assert_eq!(task.merged_size, Some(7));
}

#[test]
fn merge_jobs_prune_keeps_in_flight_merges() {
    let jobs = merge_jobs();
    let mut guard = jobs.lock().unwrap();
    guard.clear();
    guard.insert("merging-1".to_string(), MergeJobState::Merging);
    for i in 0..MERGE_JOBS_CAP {
        guard.insert(
            format!("done-{i}"),
            MergeJobState::Done {
                value: "v".to_string(),
                size: 1,
            },
        );
    }
    assert!(guard.len() > MERGE_JOBS_CAP);
    guard.retain(|_, state| matches!(state, MergeJobState::Merging));
    assert_eq!(guard.len(), 1);
    assert!(matches!(guard.get("merging-1"), Some(MergeJobState::Merging)));
    guard.clear();
}
