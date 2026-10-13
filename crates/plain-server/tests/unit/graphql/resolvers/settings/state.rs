use super::*;
use async_graphql::{EmptySubscription, Schema};
fn fixture() -> (
    tempfile::TempDir,
    Schema<PreferencesQuery, PreferencesMutation, EmptySubscription>,
    tokio::sync::broadcast::Receiver<WsEvent>,
) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    plain_rs::prefs::system::bootstrap(&prefs).unwrap();
    let (events, receiver) = broadcast::channel(32);
    let schema = Schema::build(PreferencesQuery, PreferencesMutation, EmptySubscription)
        .data(prefs)
        .data(events)
        .finish();
    (dir, schema, receiver)
}
#[tokio::test]
async fn graphql_settings_are_isolated_between_devices() {
    let (_a, first, mut events) = fixture();
    let (_b, second, _) = fixture();
    let response=first.execute("mutation { updateUserSettings(input:{editorFontSize:20}) { result { affectedCount } } }").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(events.recv().await.unwrap().event_type, "PREFS_UPDATED");
    let first = first
        .execute("{userSettings {editorFontSize}} ")
        .await
        .data
        .into_json()
        .unwrap();
    let second = second
        .execute("{userSettings {editorFontSize}} ")
        .await
        .data
        .into_json()
        .unwrap();
    assert_eq!(first["userSettings"]["editorFontSize"], 20);
    assert_eq!(second["userSettings"]["editorFontSize"], 14);
}
#[tokio::test]
async fn system_commands_validate_and_do_not_return_private_keys() {
    let (_dir, schema, _) = fixture();
    let invalid=schema.execute("mutation {updateSystemSettings(input:{action:SET_PASSWORD_TYPE,value:4}){result {affectedCount}}}").await;
    assert!(!invalid.errors.is_empty());
    let updated=schema.execute("mutation {updateSystemSettings(input:{action:SET_TWO_FACTOR,enabled:false}){result {affectedCount}}}").await;
    assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    let result = schema
        .execute("{systemSettings {authTwoFactor clientId signaturePublicKey}}")
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        result.data.into_json().unwrap()["systemSettings"]["authTwoFactor"],
        false
    );
    assert!(
        !schema
            .execute("{systemSettings {masterSecret}}")
            .await
            .errors
            .is_empty()
    );
}

#[tokio::test]
async fn runtime_system_preferences_validate_before_publishing() {
    let (_dir, schema, mut events) = fixture();
    for (key, value) in [("log_level", "\"debug\""), ("dev_token", "\"development\"")] {
        let document = format!(
            "mutation {{ setSystemPref(key: \"{key}\", value: {value}) {{ affectedCount }} }}"
        );
        let response = schema.execute(document).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(events.try_recv().unwrap().event_type, "PREFS_UPDATED");
        assert!(events.try_recv().is_err());
    }
    for document in [
        "mutation { setSystemPref(key: \"log_level\", value: \"invalid\") { affectedCount } }",
        "mutation { setSystemPref(key: \"dev_token\", value: 1) { affectedCount } }",
    ] {
        assert!(!schema.execute(document).await.errors.is_empty());
        assert!(events.try_recv().is_err());
    }
}
