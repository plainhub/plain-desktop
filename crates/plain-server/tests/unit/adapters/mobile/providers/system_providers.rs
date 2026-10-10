use super::*;
fn package(id: &str, name: &str, kind: &str, size: i64, time: &str) -> Value {
    json!({"item":{"id":id,"name":name,"type":kind,"size":size,"updatedAt":time,"certs":[{"issuer":"Certificate % issuer","subject":"subject"}]},"nameSortKey":name.to_lowercase()})
}
#[test]
fn packages_filter_certificates_types_and_page_after_sorting() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db.sqlite")).unwrap();
    let facts = vec![
        package("a", "Zeta", "USER", 5, "2026-10-05T00:00:00Z"),
        package("b", "Alpha", "SYSTEM", 9, "2026-10-05T02:00:00+02:00"),
        package("c", "Beta", "USER", 7, "2026-10-05T01:00:00Z"),
    ];
    let result = packages(&db, facts.clone(), "type:USER text:'% issuer'", "SIZE_DESC").unwrap();
    assert_eq!(page(result, 1, 1)[0]["id"], "a");
    assert_eq!(
        packages(&db, facts.clone(), "ids:a,c", "DATE_DESC").unwrap()[0]["id"],
        "c"
    );
    assert!(
        packages(&db, facts.clone(), "text:'% nonmatch'", "NAME_ASC")
            .unwrap()
            .is_empty()
    );
    assert_eq!(packages(&db, facts, "", "NAME_DESC").unwrap()[0]["id"], "b");
}
#[test]
fn notifications_apply_preferences_literal_text_and_instant_order() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    prefs
        .set_user(
            "notification_filter",
            serde_json::to_string(&json!({"mode":"allowlist","apps":["allowed"]})).unwrap(),
        )
        .unwrap();
    let facts = vec![
        json!({"id":"a","appId":"allowed","appName":"A","title":"50% sale","body":"","time":"2026-10-05T01:00:00Z"}),
        json!({"id":"b","appId":"blocked","appName":"B","title":"50% sale","body":"","time":"2026-10-05T02:00:00Z"}),
        json!({"id":"c","appId":"allowed","appName":"C","title":"50% sale","body":"","time":"2026-10-05T04:00:00+02:00"}),
    ];
    let result = notifications(&prefs, facts.clone(), "text:'50%'");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0]["id"], "c");
    assert!(notifications(&prefs, facts, "text:'50_'").is_empty());
    assert!(page(result, -1, -1).is_empty());
}
