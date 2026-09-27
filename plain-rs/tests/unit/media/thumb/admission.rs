//! Unit tests for `src/media/thumb_engine/admission.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn cfg(mb: i64) -> crate::media::config::Config {
    crate::media::config::Config::parse(&format!("[thumbnails]\nmem_budget_mb = {mb}\n"))
}

#[test]
fn mem_units_floor_semantics() {
    assert_eq!(units_for(0), 0);
    assert_eq!(units_for(MEM_UNIT - 1), 0);
    assert_eq!(units_for(MEM_UNIT), 1);
    assert_eq!(units_for(MEM_UNIT * 2 + 1), 2);
}

#[test]
fn config_clamps_budget() {
    assert_eq!(build(&cfg(1)).mem_budget_mb(), 128);
    assert_eq!(build(&cfg(999_999)).mem_budget_mb(), 16_384);
    assert_eq!(build(&cfg(2048)).mem_budget_mb(), 2048);
    assert_eq!(
        build(&crate::media::config::Config::default()).mem_budget_mb(),
        2048,
        "default budget"
    );
}

#[tokio::test]
async fn many_small_jobs_run_concurrently() {
    // Tiny memory budget but ample CPU permits: sub-unit jobs must all
    // be admissible simultaneously — memory pricing, not job counting.
    let adm = Admission::new(64, 1);
    let mut held = Vec::new();
    for _ in 0..64 {
        held.push(adm.acquire(16 * 1024).await.unwrap()); // sub-unit → free
    }
    assert_eq!(held.len(), 64);
}

#[tokio::test]
async fn oversize_single_job_rejected_fast() {
    let adm = Admission::new(4, 128); // 128 MB
    assert!(
        adm.acquire(300 * 1024 * 1024).await.is_err(),
        "300MB job must not be admitted into a 128MB budget"
    );
    // Exactly-fitting job is fine.
    assert!(adm.acquire(128 * 1024 * 1024).await.is_ok());
}

#[tokio::test]
async fn budget_blocks_until_released() {
    let adm = Admission::new(4, 1); // 1 MB
    let held = adm.acquire(1024 * 1024).await.unwrap();
    let waiter = adm.acquire(1024 * 1024);
    tokio::pin!(waiter);
    // Must be parked while the budget is exhausted...
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
            .await
            .is_err()
    );
    drop(held);
    // ...and complete once released.
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), &mut waiter)
            .await
            .is_ok()
    );
}
