use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn cancellation_is_a_commit_barrier_and_busy_or_stale_sessions_do_not_change_jobs() {
    let scans = Arc::new(MediaScans::default());
    let session = scans.begin(2).unwrap();
    assert!(scans.begin(1).is_err());
    let writes = AtomicUsize::new(0);
    session
        .record(|| {
            writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    assert_eq!(scans.cancel().unwrap().unwrap().status, Status::Cancelling);
    assert!(
        session
            .record(|| {
                writes.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .is_err()
    );
    let cancelled = session.finish(Ok(())).unwrap();
    assert_eq!(cancelled.status, Status::Cancelled);
    assert_eq!(cancelled.processed_files, 1);
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    let replacement = scans.begin(1).unwrap();
    let replacement_id = scans.snapshot().unwrap().unwrap().id;
    drop(session);
    assert_eq!(scans.snapshot().unwrap().unwrap().id, replacement_id);
    assert_eq!(scans.snapshot().unwrap().unwrap().status, Status::Running);
    drop(replacement);
    assert_eq!(scans.snapshot().unwrap().unwrap().status, Status::Cancelled);
}

#[test]
fn failed_and_completed_jobs_are_distinct_and_close_prevents_new_commits_or_scans() {
    let scans = Arc::new(MediaScans::default());
    let failed = scans.begin(2).unwrap();
    failed.record(|| Ok(())).unwrap();
    let snapshot = failed.finish(Err("probe failed".into())).unwrap();
    assert_eq!(snapshot.status, Status::Failed);
    assert!(*failed.cancel.borrow());
    assert_eq!(snapshot.error.as_deref(), Some("probe failed"));
    assert_eq!(snapshot.processed_files, 1);
    let completed = scans.begin(0).unwrap();
    assert_eq!(completed.finish(Ok(())).unwrap().status, Status::Completed);
    let active = scans.begin(1).unwrap();
    scans.close().unwrap();
    assert!(scans.begin(1).is_err());
    assert!(active.record(|| Ok(())).is_err());
    assert_eq!(active.finish(Ok(())).unwrap().status, Status::Cancelled);
}

#[tokio::test]
async fn wait_stop_observes_existing_cancellation_and_closed_channels() {
    let (sender, mut receiver) = watch::channel(true);
    wait_stop(&mut receiver).await;
    let (closed, mut receiver) = watch::channel(false);
    drop(closed);
    wait_stop(&mut receiver).await;
    drop(sender);
}

#[test]
fn directory_discovery_does_not_publish_a_guessed_total() {
    let scans = Arc::new(MediaScans::default());
    let session = scans.begin_discovery().unwrap();
    assert_eq!(scans.snapshot().unwrap().unwrap().total_files, None);
    session.record(|| Ok(())).unwrap();
    session.discovered(1).unwrap();
    let result = session.finish(Ok(())).unwrap();
    assert_eq!(result.total_files, Some(1));
    assert_eq!(result.processed_files, 1);
}
