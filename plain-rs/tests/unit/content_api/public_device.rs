use super::*;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = handler(
                &request["method"].as_str().unwrap_or_default(),
                request["params"].clone(),
            );
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

fn fixture(
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), handler);
    (
        dir,
        crate::content_api::public_schema::build(host, prefs, db),
    )
}

fn device_facts() -> Value {
    json!({
        "name": "Pixel 8", "platform": "ANDROID", "manufacturer": "Google", "model": "shiba",
        "osName": "Android", "osVersion": "14", "kernelVersion": "6.1",
        "appVersion": "1.2.3", "appBuildNumber": "45", "language": "en",
        "cpuArch": "arm64-v8a", "cpuModel": Value::Null,
        "totalMemory": 8000000000i64, "totalStorage": 128000000000i64,
        "display": {"width": 1080, "height": 2400, "density": 2.75},
        "android": {
            "sdkVersion": 34, "versionCodeName": "REL", "securityPatch": "2024-01-01",
            "bootloader": "b", "fingerprint": "fp", "hardware": "hw", "radioVersion": "rv",
            "board": "bd", "buildBrand": "google", "buildNumber": "UQ1A", "device": "shiba",
            "javaVmVersion": "17", "glEsVersion": "3.2",
            "buildTime": "2024-01-01T00:00:00Z",
        },
    })
}

#[tokio::test]
async fn device_info_carries_the_hardware_report() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDeviceInfoFacts" => device_facts(),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { deviceInfo { name platform manufacturer model osVersion cpuArch
                        cpuModel totalMemory totalStorage
                        display { width height density }
                        android { sdkVersion buildBrand buildTime } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let info = response.data.into_json().unwrap()["deviceInfo"].clone();
    assert_eq!(info["name"], "Pixel 8");
    assert_eq!(info["platform"], "ANDROID");
    assert_eq!(info["totalMemory"], 8000000000i64);
    assert_eq!(info["cpuModel"], Value::Null);
    assert_eq!(info["display"]["density"], 2.75);
    assert_eq!(info["android"]["sdkVersion"], 34);
    assert_eq!(info["android"]["buildBrand"], "google");
    assert_eq!(info["android"]["buildTime"], "2024-01-01T00:00:00.000Z");
}

/// The Android block and the display are platform-specific; on anything
/// else they are absent, not zeroed.
#[tokio::test]
async fn a_non_android_report_omits_the_android_extras() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDeviceInfoFacts" => {
            let mut facts = device_facts();
            facts["platform"] = json!("IOS");
            facts["android"] = Value::Null;
            facts["display"] = Value::Null;
            facts
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { deviceInfo { platform display { width } android { sdkVersion } } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let info = response.data.into_json().unwrap()["deviceInfo"].clone();
    assert_eq!(info["platform"], "IOS");
    assert_eq!(info["display"], Value::Null);
    assert_eq!(info["android"], Value::Null);
}

/// Battery and memory are "the platform cannot tell"; CPU load and free
/// storage are real readings and a zero there is meaningful.
#[tokio::test]
async fn unreadable_readings_are_null_but_real_zeroes_are_zero() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDeviceStatusFacts" => json!({
            "uptimeSec": 3600, "batteryLevel": Value::Null, "charging": false,
            "temperatures": [{"label": "cpu", "celsius": 41.5}],
            "cpuUsage": 0.0, "memoryAvailable": Value::Null, "storageAvailable": 0,
        }),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { deviceStatus { uptimeSec batteryLevel charging cpuUsage
                        memoryAvailable storageAvailable temperatures { label celsius } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let status = response.data.into_json().unwrap()["deviceStatus"].clone();
    assert_eq!(status["uptimeSec"], 3600);
    assert_eq!(status["batteryLevel"], Value::Null);
    assert_eq!(status["memoryAvailable"], Value::Null);
    assert_eq!(status["cpuUsage"], 0.0);
    assert_eq!(status["storageAvailable"], 0);
    assert_eq!(status["temperatures"][0]["celsius"], 41.5);
}

#[tokio::test]
async fn the_app_root_exposes_the_identity_and_the_capability_lists() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemAppFacts" => json!({
            "clientId": "c1", "urlToken": "dG9rZW4=", "httpPort": 8080, "httpsPort": 8443,
            "appDir": "/data/user/0/com.ismartcoding.plain", "deviceName": "Pixel 8",
            "deviceType": "PHONE", "capabilities": ["SMS", "CONTACTS", "WAT"],
            "buildChannel": "GITHUB", "permissions": ["READ_SMS", "NOPE"],
            "downloadsDir": "/storage/emulated/0/Download", "developerMode": true, "debug": false,
        }),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { app { clientId urlToken httpPort httpsPort deviceName deviceType
                        buildChannel capabilities permissions developerMode debug } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let app = response.data.into_json().unwrap()["app"].clone();
    assert_eq!(app["clientId"], "c1");
    assert_eq!(app["httpPort"], 8080);
    assert_eq!(app["deviceType"], "PHONE");
    assert_eq!(app["buildChannel"], "GITHUB");
    assert_eq!(app["developerMode"], true);
    assert_eq!(app["capabilities"], json!(["SMS", "CONTACTS"]));
    assert_eq!(app["permissions"], json!(["READ_SMS"]));
}

/// A capability or permission this build has never heard of is dropped
/// rather than invented — a client cannot render a section it cannot
/// drive, and cannot request a permission it has no name for.
#[test]
fn unknown_capabilities_and_permissions_are_dropped() {
    assert_eq!(Capability::parse("SMS"), Some(Capability::Sms));
    assert_eq!(Capability::parse("POMODORO"), Some(Capability::Pomodoro));
    assert_eq!(Capability::parse("TELEPORT"), None);
    assert_eq!(Permission::parse("READ_SMS"), Some(Permission::ReadSms));
    assert_eq!(Permission::parse("ADB"), Some(Permission::Adb));
    assert_eq!(Permission::parse("NOT_A_PERMISSION"), None);
}

/// A search must not be cut short by the page size: the platform filters
/// the whole buffer and only then applies offset/limit.
#[tokio::test]
async fn a_log_search_passes_the_filter_and_the_page_separately() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemAppLogFacts" => {
            assert_eq!(params["query"], "boom");
            assert_eq!(params["offset"], 5);
            assert_eq!(params["limit"], 10);
            json!({ "lines": ["e boom 6", "e boom 7"] })
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { appLogs(offset:5, limit:10, query:"text:boom size:>1") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["appLogs"],
        json!(["e boom 6", "e boom 7"])
    );
}

/// A query with no `text` field filters on nothing — the log view pages the
/// whole buffer.
#[tokio::test]
async fn a_log_query_without_text_sends_an_empty_filter() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemAppLogFacts" => {
            assert_eq!(params["query"], "");
            json!({ "lines": [] })
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { appLogs(offset:0, limit:10, query:"size:>1") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn the_log_path_is_asked_for_on_its_own() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemAppLogFacts" => {
            assert_eq!(params["pathOnly"], true);
            json!({ "path": "/data/logs/plain.log" })
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"query { appLogPath }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["appLogPath"],
        "/data/logs/plain.log"
    );
}

#[tokio::test]
async fn clearing_the_logs_reports_success() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemClearAppLogs" => json!(true),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"mutation { clearAppLogs }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["clearAppLogs"], true);
}

#[tokio::test]
async fn a_temp_value_is_echoed_rather_than_read_back() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemSetTempValue" => {
            assert_eq!(params["key"], "k");
            assert_eq!(params["value"], "v");
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { setTempValue(key:"k", value:"v") { key value } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let pair = response.data.into_json().unwrap()["setTempValue"].clone();
    assert_eq!(pair["key"], "k");
    assert_eq!(pair["value"], "v");
}

#[tokio::test]
async fn discovery_state_and_commands_are_separate_calls() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDiscoveryFacts" => json!({ "scanning": true }),
        "systemStartDiscovery" | "systemStopDiscovery" => json!(true),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"query { isDiscovering }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["isDiscovering"], true);

    let response = schema
        .execute(r#"mutation { startDiscovery stopDiscovery }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["startDiscovery"], true);
    assert_eq!(data["stopDiscovery"], true);
}

#[tokio::test]
async fn renaming_the_device_forwards_the_new_name() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemUpdateDeviceName" => {
            assert_eq!(params["name"], "New Name");
            json!(true)
        }
        "systemRelaunchApp" => json!(true),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { updateDeviceName(name:"New Name") relaunchApp }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["updateDeviceName"], true);
    assert_eq!(data["relaunchApp"], true);
}
