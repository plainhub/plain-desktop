use super::*;
use crate::chat::enums::{DeviceType, PeerStatus};
use std::sync::atomic::{AtomicUsize, Ordering};

fn db() -> Db {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let mut peer = DPeer::new("peer", "Fixture", "", 1, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = crate::base64_encode(&[7; 32]);
    peer.public_key = crate::base64_encode(&[8; 32]);
    db.upsert_peer(&peer);
    db
}
struct Mock {
    local_aware: bool,
    remote_aware: bool,
    running: bool,
    starts: bool,
    wrong_id: bool,
    unpair_scan: Option<Db>,
    unpair_start: Option<Db>,
    entered: Option<Arc<tokio::sync::Notify>>,
    start_calls: AtomicUsize,
    observations: Mutex<Vec<Advertisement>>,
}
impl Default for Mock {
    fn default() -> Self {
        Self {
            local_aware: true,
            remote_aware: true,
            running: false,
            starts: true,
            wrong_id: false,
            unpair_scan: None,
            unpair_start: None,
            entered: None,
            start_calls: AtomicUsize::new(0),
            observations: Mutex::new(Vec::new()),
        }
    }
}
impl Driver for Mock {
    async fn capabilities(&self) -> Result<Capabilities> {
        Ok(Capabilities {
            ble_ready: true,
            aware_supported: self.local_aware,
        })
    }
    async fn scan(&self, short_id: &str) -> Result<Option<Advertisement>> {
        assert_eq!(short_id, "2ffc1d06387ef8bb");
        if let Some(db) = &self.unpair_scan {
            peers::unpair(db, "peer")?;
        }
        if let Some(entered) = &self.entered {
            entered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(Some(Advertisement {
            short_id: if self.wrong_id {
                "wrong".into()
            } else {
                short_id.into()
            },
            aware_supported: self.remote_aware,
            aware_running: self.running,
        }))
    }
    async fn start_aware(&self, _: &str) -> Result<bool> {
        self.start_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(db) = &self.unpair_start {
            peers::unpair(db, "peer")?;
        }
        Ok(self.starts)
    }
    async fn observe(&self, _: &str, value: &Advertisement) -> Result<()> {
        self.observations.lock().unwrap().push(value.clone());
        Ok(())
    }
}
#[test]
fn monotonic_throttle_capacity_and_old_claim_cannot_release_new_generation() {
    let registry = Arc::new(Prewarmer::default());
    let now = Instant::now();
    let old = registry.begin("peer", now).unwrap().unwrap();
    assert!(
        registry
            .begin("peer", now + Duration::from_secs(31))
            .unwrap()
            .is_none()
    );
    registry.forget("peer");
    let current = registry.begin("peer", now).unwrap().unwrap();
    assert!(!old.current());
    drop(old);
    assert!(current.current());
    drop(current);
    assert!(
        registry
            .begin("peer", now + Duration::from_secs(29))
            .unwrap()
            .is_none()
    );
    assert!(
        registry
            .begin("peer", now + Duration::from_secs(30))
            .unwrap()
            .is_some()
    );
    for i in 0..127 {
        drop(registry.begin(&format!("p{i}"), now).unwrap().unwrap());
    }
    assert!(registry.begin("full", now).is_err());
    assert!(
        registry
            .begin("expired", now + Duration::from_secs(601))
            .unwrap()
            .is_some()
    );
}
#[tokio::test]
async fn actual_false_receipt_never_marks_running_and_capability_gates_avoid_send() {
    for (local, remote, running, starts, expected_calls, expected_running) in [
        (true, true, false, false, 1, false),
        (true, true, false, true, 1, true),
        (false, true, false, true, 0, false),
        (true, false, false, true, 0, false),
        (true, true, true, true, 0, true),
    ] {
        let db = db();
        let registry = Arc::new(Prewarmer::default());
        let driver = Mock {
            local_aware: local,
            remote_aware: remote,
            running,
            starts,
            ..Mock::default()
        };
        let result = run(&db, &registry, "peer", &driver).await.unwrap().unwrap();
        assert_eq!(result.aware_running, expected_running);
        assert_eq!(driver.start_calls.load(Ordering::SeqCst), expected_calls);
        assert_eq!(
            driver
                .observations
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .aware_running,
            expected_running
        );
        assert!(
            run(&db, &registry, "peer", &driver)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(driver.start_calls.load(Ordering::SeqCst), expected_calls);
    }
}
#[tokio::test]
async fn unknown_unpaired_mismatched_and_deleted_peers_do_not_accept_late_observations() {
    let db = db();
    let registry = Arc::new(Prewarmer::default());
    let driver = Mock::default();
    assert!(
        run(&db, &registry, "missing", &driver)
            .await
            .unwrap()
            .is_none()
    );
    peers::unpair(&db, "peer").unwrap();
    assert!(
        run(&db, &registry, "peer", &driver)
            .await
            .unwrap()
            .is_none()
    );
    assert!(driver.observations.lock().unwrap().is_empty());
    let db = self::db();
    let driver = Mock {
        wrong_id: true,
        ..Mock::default()
    };
    assert!(run(&db, &registry, "peer", &driver).await.is_err());
    assert!(driver.observations.lock().unwrap().is_empty());
    registry.forget("peer");
    let driver = Mock {
        unpair_scan: Some(db.clone()),
        ..Mock::default()
    };
    assert!(
        run(&db, &registry, "peer", &driver)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(driver.start_calls.load(Ordering::SeqCst), 0);
    assert!(driver.observations.lock().unwrap().is_empty());
    let db = self::db();
    registry.forget("peer");
    let driver = Mock {
        unpair_start: Some(db.clone()),
        ..Mock::default()
    };
    assert!(
        run(&db, &registry, "peer", &driver)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        driver
            .observations
            .lock()
            .unwrap()
            .iter()
            .all(|v| !v.aware_running)
    );
}
#[tokio::test]
async fn concurrent_requests_and_cancellation_release_only_the_active_claim() {
    let db = db();
    let registry = Arc::new(Prewarmer::default());
    let entered = Arc::new(tokio::sync::Notify::new());
    let driver = Arc::new(Mock {
        entered: Some(entered.clone()),
        ..Mock::default()
    });
    let task = tokio::spawn({
        let db = db.clone();
        let registry = registry.clone();
        let driver = driver.clone();
        async move { run(&db, &registry, "peer", &*driver).await }
    });
    entered.notified().await;
    assert!(
        run(&db, &registry, "peer", &*driver)
            .await
            .unwrap()
            .is_none()
    );
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!registry.state.lock().unwrap().entries["peer"].active);
    assert!(driver.observations.lock().unwrap().is_empty());
    assert_eq!(registry.slots.available_permits(), 2);
}

#[tokio::test]
async fn busy_prewarm_returns_without_scan_or_throttling_the_waiting_peer() {
    let db = db();
    let registry = Arc::new(Prewarmer::default());
    let driver = Mock::default();
    let first = registry.slots.acquire().await.unwrap();
    let second = registry.slots.acquire().await.unwrap();
    assert!(
        run(&db, &registry, "peer", &driver)
            .await
            .unwrap()
            .is_none()
    );
    assert!(registry.state.lock().unwrap().entries.is_empty());
    assert!(driver.observations.lock().unwrap().is_empty());
    drop(first);
    drop(second);
    assert!(
        run(&db, &registry, "peer", &driver)
            .await
            .unwrap()
            .is_some()
    );
}
