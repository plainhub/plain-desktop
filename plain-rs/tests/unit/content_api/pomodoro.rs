use super::*;
fn expire(db: &Db) {
    db.with_conn(|c| {
        let raw: String = c
            .query_row("SELECT data FROM pomodoro_runtime WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let mut state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        state["deadline"] = serde_json::json!("2000-01-01T00:00:00Z");
        c.execute(
            "UPDATE pomodoro_runtime SET data=?1 WHERE id=1",
            [state.to_string()],
        )
        .unwrap();
    });
}
#[tokio::test]
async fn pomodoro_host_ticks_statistics_atomic_completion_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let db = Arc::new(Db::open(&path).unwrap());
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set_user("pomodoro_settings",&r#"{"workDurationMin":1,"shortBreakDurationMin":2,"longBreakDurationMin":3,"pomodorosBeforeLongBreak":2}"#).unwrap();
    let token = crate::base64_encode(&[12; 32]);
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let config = r#"mutation {configurePomodoroDay(date:"2026-10-03",dayStart:"2026-10-02T16:00:00Z") {date timeLeftSec}}"#;
    let result = call(server.port, &token, config).await;
    assert!(result.get("errors").is_none(), "{result}");
    assert_eq!(result["data"]["configurePomodoroDay"]["timeLeftSec"], 60);
    for bad in [
        "mutation {startPomodoro(durationSec:0)}",
        "mutation {adjustPomodoro(durationSec:-1)}",
    ] {
        assert!(call(server.port, &token, bad).await.get("errors").is_some());
    }
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&db);
    let query = "mutation {tickPomodoro {today {completedCount state timeLeftSec} completedState}}";
    let results =
        futures_util::future::join_all((0..16).map(|_| call(server.port, &token, query))).await;
    assert_eq!(
        results
            .iter()
            .filter(|r| r["data"]["tickPomodoro"]["completedState"] == "WORK")
            .count(),
        1
    );
    let row = db.pomodoro_get_by_date("2026-10-03").unwrap().unwrap();
    assert_eq!(row.completed_count, 1);
    assert_eq!(row.total_work_seconds, 60);
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&db);
    call(server.port, &token, query).await;
    let row = db.pomodoro_get_by_date("2026-10-03").unwrap().unwrap();
    assert_eq!(row.total_break_seconds, 120);
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&db);
    let done = call(server.port, &token, query).await;
    assert_eq!(done["data"]["tickPomodoro"]["today"]["state"], "LONG_BREAK");
    assert_eq!(done["data"]["tickPomodoro"]["today"]["timeLeftSec"], 180);
    call(
        server.port,
        &token,
        "mutation {skipPomodoro {completedState}}",
    )
    .await;
    assert_eq!(
        db.pomodoro_get_by_date("2026-10-03")
            .unwrap()
            .unwrap()
            .total_break_seconds,
        120
    );
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:30)}",
    )
    .await;
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    assert_eq!(
        call(
            server.port,
            &token,
            "{pomodoroToday {isRunning completedCount}}"
        )
        .await["data"]["pomodoroToday"]["isRunning"],
        true
    );
    call(server.port, &token, "mutation {pausePomodoro}").await;
    assert!(
        call(server.port, &token, query).await["data"]["tickPomodoro"]["completedState"].is_null()
    );
    call(server.port,&token,r#"mutation {configurePomodoroDay(date:"2026-10-04",dayStart:"2026-10-03T16:00:00Z") {completedCount}}"#).await;
    assert_eq!(
        call(server.port, &token, "{pomodoroToday {completedCount}}").await["data"]["pomodoroToday"]
            ["completedCount"],
        0
    );
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&db);
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER reject_runtime BEFORE UPDATE ON pomodoro_runtime BEGIN SELECT RAISE(ABORT, 'test write failure'); END;").unwrap());
    assert!(
        call(server.port, &token, query)
            .await
            .get("errors")
            .is_some()
    );
    assert!(db.pomodoro_get_by_date("2026-10-04").unwrap().is_none());
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_runtime;").unwrap());
    assert_eq!(
        call(server.port, &token, query).await["data"]["tickPomodoro"]["today"]["completedCount"],
        1
    );
    let history=call(server.port,&token,r#"{pomodoroRecords(startDate:"2026-10-03") {date totalWorkSec totalBreakSec createdAt} pomodoroTotalCompleted pomodoroRecord(date:"2026-10-04") {completedCount}}"#).await;
    assert!(history.get("errors").is_none(), "{history}");
    assert_eq!(history["data"]["pomodoroTotalCompleted"], 3);
    assert_eq!(
        history["data"]["pomodoroRecords"].as_array().unwrap().len(),
        2
    );
    assert_eq!(history["data"]["pomodoroRecord"]["completedCount"], 1);
    server.shutdown().await;
}
#[test]
fn pomodoro_ticks_do_not_invalidate_other_domains() {
    for query in [
        "mutation { tickPomodoro { completedState } }",
        "mutation { named: tickPomodoro {completedState} }",
        "query {pomodoroToday {state}}",
    ] {
        assert!(!content_changes(&Request::new(query)));
    }
    assert!(content_changes(&Request::new(
        "mutation {tickPomodoro {completedState} deleteBookmarks(ids:[]) {affectedCount}}"
    )));
}
