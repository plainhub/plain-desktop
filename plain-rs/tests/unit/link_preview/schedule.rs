use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
#[tokio::test]
async fn schedule_caps_jobs_cancels_outdated_work_and_reruns_latest_request() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let max = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    struct Guard(Arc<AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let fetch: Fetch = {
        let active = active.clone();
        let max = max.clone();
        let calls = calls.clone();
        Arc::new(move |_, _, _| {
            let active = active.clone();
            let max = max.clone();
            let calls = calls.clone();
            Box::pin(async move {
                let _guard = Guard(active.clone());
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(current, Ordering::SeqCst);
                calls.fetch_add(1, Ordering::SeqCst);
                futures_util::future::pending::<()>().await;
                Ok(None)
            })
        })
    };
    let (stop, stopped) = watch::channel(false);
    let scheduler = Schedule::with_fetch(
        db,
        dir.path().to_path_buf(),
        Arc::new(|_| panic!("canceled fetch must not publish")),
        stopped,
        fetch,
    );
    for n in 0..128 {
        assert!(scheduler.request(&n.to_string()));
    }
    assert!(!scheduler.request("overflow"));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while calls.load(Ordering::SeqCst) < 4 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(active.load(Ordering::SeqCst), 4);
    assert!(scheduler.request("0"));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while calls.load(Ordering::SeqCst) < 5 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(max.load(Ordering::SeqCst) <= 4);
    stop.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
