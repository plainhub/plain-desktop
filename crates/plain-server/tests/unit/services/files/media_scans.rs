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

#[test]
fn completion_records_share_the_close_barrier_without_occupying_the_directory_job() {
    let scans = Arc::new(MediaScans::default());
    let directory = scans.begin_discovery().unwrap();
    let writes = AtomicUsize::new(0);
    scans
        .record_completion(|| {
            writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    assert_eq!(scans.snapshot().unwrap().unwrap().status, Status::Running);
    assert_eq!(scans.snapshot().unwrap().unwrap().processed_files, 0);
    scans.close().unwrap();
    assert!(
        scans
            .record_completion(|| {
                writes.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .is_err()
    );
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    drop(directory);
}

#[tokio::test]
async fn pause_is_a_commit_barrier_and_resume_keeps_the_same_session() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    let scans = Arc::new(MediaScans::default());
    let session = scans.begin(1).unwrap();
    let id = scans.pause().unwrap().unwrap().id;
    assert_eq!(scans.snapshot().unwrap().unwrap().status, Status::Paused);
    assert!(scans.begin(1).is_err());
    let writes = AtomicUsize::new(0);
    let mut record = Box::pin(session.record_when_running(|| {
        writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(record.as_mut().poll(&mut context), Poll::Pending));
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(scans.resume().unwrap().unwrap().id, id);
    record.await.unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    let completed = session.finish(Ok(())).unwrap();
    assert_eq!(completed.id, id);
    assert_eq!(completed.processed_files, 1);
    assert_eq!(completed.status, Status::Completed);
    assert_eq!(scans.resume().unwrap().unwrap().status, Status::Completed);
}

#[tokio::test]
async fn cancelling_or_closing_a_paused_scan_wakes_it_without_a_write() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    for close in [false, true] {
        let scans = Arc::new(MediaScans::default());
        let session = scans.begin(1).unwrap();
        scans.pause().unwrap();
        let writes = AtomicUsize::new(0);
        let mut record = Box::pin(session.record_when_running(|| {
            writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(record.as_mut().poll(&mut context), Poll::Pending));
        if close {
            scans.close().unwrap();
        } else {
            scans.cancel().unwrap();
        }
        assert!(record.await.is_err());
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        assert_eq!(session.finish(Ok(())).unwrap().status, Status::Cancelled);
    }
}

#[tokio::test]
async fn background_session_is_reserved_before_running_and_shutdown_joins_it() {
    let scans = Arc::new(MediaScans::default());
    let session = scans.begin_discovery().unwrap();
    let (started, running) = tokio::sync::oneshot::channel();
    let snapshot = scans
        .spawn(
            &tokio::runtime::Handle::current(),
            session,
            move |session| {
                Box::pin(async move {
                    started.send(()).unwrap();
                    let mut cancel = session.cancel.clone();
                    wait_stop(&mut cancel).await;
                    session.finish(Err("cancelled".into())).unwrap();
                })
            },
        )
        .unwrap();
    assert_eq!(snapshot.status, Status::Running);
    assert!(scans.begin_discovery().is_err());
    running.await.unwrap();
    scans.close().unwrap();
    scans.wait_background().await.unwrap();
    assert_eq!(scans.snapshot().unwrap().unwrap().status, Status::Cancelled);
    assert!(scans.begin_discovery().is_err());
}

#[test]
fn core_scan_events_report_real_lifecycle_and_unknown_totals() {
    let (events, mut received) = tokio::sync::broadcast::channel(32);
    let scans = Arc::new(MediaScans::new(events));
    let session = scans.begin_discovery().unwrap();
    let event = received.try_recv().unwrap();
    assert_eq!(event.event_type, plain_rs::events::MEDIA_SCAN_PROGRESS);
    let first: serde_json::Value = serde_json::from_str(&event.payload).unwrap();
    assert_eq!(first["state"], "RUNNING");
    assert_eq!(first["total"], serde_json::Value::Null);
    assert_eq!(first["pending"], serde_json::Value::Null);
    scans.pause().unwrap();
    let paused: serde_json::Value =
        serde_json::from_str(&received.try_recv().unwrap().payload).unwrap();
    assert_eq!(paused["state"], "PAUSED");
    assert_eq!(paused["id"], first["id"]);
    scans.resume().unwrap();
    assert!(received.try_recv().is_ok());
    session.record(|| Ok(())).unwrap();
    let progress: serde_json::Value =
        serde_json::from_str(&received.try_recv().unwrap().payload).unwrap();
    assert_eq!(progress["indexed"], 1);
    session.finish(Err("probe failed".into())).unwrap();
    let failed: serde_json::Value =
        serde_json::from_str(&received.try_recv().unwrap().payload).unwrap();
    assert_eq!(failed["state"], "FAILED");
    assert_eq!(failed["error"], "probe failed");
    assert_eq!(failed["total"], serde_json::Value::Null);
}
