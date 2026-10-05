use super::*;
fn intent(kind: Kind) -> Intent {
    Intent {
        message_id: "message".into(),
        kind,
        link: Link::new("127.0.0.1", 8443, "share", &crate::base64_encode(&[7; 32])).unwrap(),
        url_token: crate::base64_encode(&[8; 32]),
        entries: vec![File {
            name: "fixture.txt".into(),
            virtual_path: "fixture.txt".into(),
            is_dir: false,
            size: 3,
            mime_type: "text/plain".into(),
            has_thumb: false,
        }],
        target_dir: "/downloads".into(),
        downloads_base: "/downloads".into(),
        zip_name: "fixture.zip".into(),
    }
}
fn planned(q: &Queue, run: &Run) {
    let mut walker = Walker::new(
        run.intent.kind,
        run.intent.entries.clone(),
        "/downloads",
        "/downloads",
    )
    .unwrap();
    assert!(walker.next_directory().unwrap().is_none());
    let plan = walker.finish().unwrap();
    q.planned(&run.snapshot.id, &run.snapshot.generation, plan)
        .unwrap();
}
#[test]
fn fifo_limit_duplicates_and_retry_skip_confirmed_files() {
    let q = Queue::default();
    let mut ids = vec![];
    for n in 0..4 {
        let mut i = intent(Kind::File);
        i.message_id = n.to_string();
        ids.push(q.enqueue(i).unwrap());
    }
    let runs = (0..3)
        .map(|_| q.claim(&HashSet::new()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        runs.iter().map(|r| &r.snapshot.id).collect::<Vec<_>>(),
        ids[..3].iter().collect::<Vec<_>>()
    );
    assert!(q.claim(&HashSet::new()).is_none());
    assert_eq!(q.enqueue(runs[0].intent.clone()).unwrap(), ids[0]);
    planned(&q, &runs[0]);
    q.file_finished(&ids[0], &runs[0].snapshot.generation, "fixture.txt", None)
        .unwrap();
    q.finish(&ids[0], &runs[0].snapshot.generation, None);
    assert_eq!(q.claim(&HashSet::new()).unwrap().snapshot.id, ids[3]);
    q.control(&ids[0], "retry").unwrap();
    assert!(q.claim(&HashSet::new()).is_none());
    q.control(&ids[1], "cancel").unwrap();
    let retry = q.claim(&HashSet::new()).unwrap();
    assert!(retry.completed.contains("fixture.txt"));
    assert_ne!(retry.snapshot.generation, runs[0].snapshot.generation);
}
#[tokio::test]
async fn receipts_are_generation_and_ticket_scoped_and_validate_real_byte_count() {
    let q = Queue::default();
    let id = q.enqueue(intent(Kind::File)).unwrap();
    let run = q.claim(&HashSet::new()).unwrap();
    planned(&q, &run);
    let (ticket, rx) = q
        .begin(&id, &run.snapshot.generation, "fixture.txt", Some(3))
        .unwrap();
    assert!(
        q.progress(&id, &run.snapshot.generation, &ticket, 2)
            .unwrap()
    );
    assert!(
        q.progress(&id, &run.snapshot.generation, &ticket, 1)
            .is_err()
    );
    assert!(
        q.progress(&id, &run.snapshot.generation, &ticket, 4)
            .is_err()
    );
    assert!(
        !q.receipt(
            &id,
            &run.snapshot.generation,
            "stale",
            Receipt {
                path: "/downloads/fixture.txt".into(),
                bytes: 3,
                error: None
            }
        )
        .unwrap()
    );
    assert!(
        !q.receipt(
            &id,
            &run.snapshot.generation,
            &ticket,
            Receipt {
                path: "/downloads/fixture.txt".into(),
                bytes: 2,
                error: None
            }
        )
        .unwrap()
    );
    assert!(rx.await.unwrap().error.is_some());
    q.file_finished(
        &id,
        &run.snapshot.generation,
        "fixture.txt",
        Some("short read".into()),
    )
    .unwrap();
    q.finish(&id, &run.snapshot.generation, None);
    let snap = q.snapshot();
    assert_eq!(snap["tasks"][0]["status"], "FAILED");
    assert_eq!(snap["tasks"][0]["downloadedSize"], 0);
    q.control(&id, "retry").unwrap();
    let fresh = q.claim(&HashSet::new()).unwrap();
    let (new, rx) = q
        .begin(&id, &fresh.snapshot.generation, "fixture.txt", Some(3))
        .unwrap();
    assert!(
        !q.receipt(
            &id,
            &run.snapshot.generation,
            &ticket,
            Receipt {
                path: "/old".into(),
                bytes: 3,
                error: None
            }
        )
        .unwrap()
    );
    q.control(&id, "pause").unwrap();
    assert!(rx.await.is_err());
    assert!(
        !q.progress(&id, &fresh.snapshot.generation, &new, 3)
            .unwrap()
    );
    q.control(&id, "resume").unwrap();
    let final_run = q.claim(&HashSet::new()).unwrap();
    assert_ne!(fresh.snapshot.generation, final_run.snapshot.generation);
}
#[test]
fn zip_failure_rolls_back_success_counters_and_blocked_cleanup_cannot_relaunch() {
    let q = Queue::default();
    let id = q.enqueue(intent(Kind::Zip)).unwrap();
    let run = q.claim(&HashSet::new()).unwrap();
    planned(&q, &run);
    q.file_finished(&id, &run.snapshot.generation, "fixture.txt", None)
        .unwrap();
    q.finish(
        &id,
        &run.snapshot.generation,
        Some("OS refused archive".into()),
    );
    assert_eq!(q.snapshot()["tasks"][0]["status"], "FAILED");
    assert_eq!(q.snapshot()["tasks"][0]["doneFiles"], 0);
    q.enqueue(run.intent.clone()).unwrap();
    assert!(q.claim(&HashSet::from([id.clone()])).is_none());
    let retry = q.claim(&HashSet::new()).unwrap();
    assert!(retry.completed.is_empty());
    q.stop();
    assert!(q.enqueue(intent(Kind::File)).is_err());
    assert!(q.claim(&HashSet::new()).is_none());
    assert_eq!(q.snapshot()["tasks"][0]["status"], "CANCELED");
}

#[test]
fn partial_failures_keep_only_confirmed_bytes_and_retries_resume_remaining_paths() {
    let q = Queue::default();
    let mut i = intent(Kind::Multi);
    let mut second = i.entries[0].clone();
    second.name = "second.txt".into();
    second.virtual_path = "second.txt".into();
    second.size = 4;
    i.entries.push(second);
    let id = q.enqueue(i).unwrap();
    let run = q.claim(&HashSet::new()).unwrap();
    planned(&q, &run);
    q.file_finished(&id, &run.snapshot.generation, "fixture.txt", None)
        .unwrap();
    q.file_finished(
        &id,
        &run.snapshot.generation,
        "second.txt",
        Some("OS refused".into()),
    )
    .unwrap();
    q.finish(&id, &run.snapshot.generation, None);
    let s = q.snapshot();
    assert_eq!(s["tasks"][0]["status"], "PARTIAL");
    assert_eq!(s["tasks"][0]["doneFiles"], 1);
    assert_eq!(s["tasks"][0]["failedFiles"], 1);
    assert_eq!(s["tasks"][0]["downloadedSize"], 3);
    q.control(&id, "retry").unwrap();
    let retry = q.claim(&HashSet::new()).unwrap();
    planned(&q, &retry);
    assert!(retry.completed.contains("fixture.txt"));
    q.file_finished(&id, &retry.snapshot.generation, "second.txt", None)
        .unwrap();
    q.finish(&id, &retry.snapshot.generation, None);
    let s = q.snapshot();
    assert_eq!(s["tasks"][0]["status"], "COMPLETED");
    assert_eq!(s["tasks"][0]["downloadedSize"], 7);
    assert_eq!(s["tasks"][0]["failedFiles"], 0);
}
