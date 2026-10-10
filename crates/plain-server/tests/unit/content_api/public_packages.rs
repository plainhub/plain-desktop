use super::*;
use crate::content_api::public_schema::PublicSchema;
use serde_json::json;

/// Answers every host call from `handler`, so a resolver test exercises the
/// real `Host` round trip (including the permission pref it reads).
fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
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

fn fixture(permissions: Value) -> (tempfile::TempDir, PublicSchema, Arc<Prefs>) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("api_permissions", permissions).unwrap();
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let host = Arc::new(Host::default());
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs.clone(), db, directory),
        prefs,
    )
}

fn fact(id: &str, kind: &str, size: i64) -> Value {
    json!({"item":{
        "id":id,"name":id,"type":kind,"version":"1.0","path":"/data/app.apk","size":size,
        "installedAt":"2026-10-01T00:00:00Z","updatedAt":"2026-10-05T00:00:00Z",
        "certs":[{"issuer":"issuer","subject":"subject","serialNumber":"0x1",
                  "validFrom":"2026-01-01T00:00:00Z","validTo":"2036-01-01T00:00:00Z"}],
    },"nameSortKey":id})
}

#[tokio::test]
async fn packages_maps_host_facts_into_the_contract_types() {
    let (dir, schema, _prefs) = fixture(json!(["QUERY_ALL_PACKAGES"]));
    let host = schema_host(&schema);
    stub(host.clone(), |method, _| match method {
        "systemPackageFacts" => json!([fact("com.b", "SYSTEM", 20), fact("com.a", "USER", 10),]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { packages(offset:0, limit:10, query:"", sortBy:SIZE_DESC) {
                   id name type version path size installedAt updatedAt
                   certs { issuer subject serialNumber validFrom validTo } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let packages = response.data.into_json().unwrap()["packages"].clone();
    assert_eq!(packages[0]["id"], "com.b");
    assert_eq!(packages[0]["type"], "SYSTEM");
    assert_eq!(packages[0]["size"], 20);
    assert_eq!(packages[1]["type"], "USER");
    assert_eq!(packages[0]["certs"][0]["serialNumber"], "0x1");
    assert_eq!(
        packages[0]["certs"][0]["validTo"],
        "2036-01-01T00:00:00.000Z"
    );
    drop(dir);
}

#[tokio::test]
async fn package_roots_error_out_without_the_api_permission() {
    let (_dir, schema, _prefs) = fixture(json!(["READ_CONTACTS"]));
    let response = schema.execute(r#"query { packageCount(query:"") }"#).await;
    // plain-app degrades packageCount to 0 instead of erroring.
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["packageCount"], 0);

    let response = schema
        .execute(r#"query { packages(offset:0, limit:1, query:"", sortBy:NAME_ASC) { id } }"#)
        .await;
    assert_eq!(response.errors[0].message, "no_permission");
}

#[tokio::test]
async fn package_count_requires_the_platform_grant_too() {
    let (_dir, schema, _prefs) = fixture(json!(["QUERY_ALL_PACKAGES"]));
    let host = schema_host(&schema);
    stub(host.clone(), |method, _| match method {
        "systemPackageFacts" => json!([fact("com.a", "USER", 1), fact("com.b", "USER", 2)]),
        "systemPermissionFacts" => json!({"granted":{"QUERY_ALL_PACKAGES": false}}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"query { packageCount(query:"") }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["packageCount"], 0);
}

#[tokio::test]
async fn package_statuses_keep_missing_packages_in_the_reply() {
    let (_dir, schema, _prefs) = fixture(json!(["QUERY_ALL_PACKAGES"]));
    let host = schema_host(&schema);
    stub(host.clone(), |method, params| match method {
        "systemPackageStatuses" => {
            assert_eq!(params["ids"], json!(["gone", "com.a"]));
            json!({"gone": Value::Null, "com.a": {"updatedAt":"2026-10-05T00:00:00Z"}})
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { packageStatuses(ids:["gone","com.a"]) { id exists updatedAt } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let statuses = response.data.into_json().unwrap()["packageStatuses"].clone();
    assert_eq!(statuses[0]["id"], "gone");
    assert_eq!(statuses[0]["exists"], false);
    assert_eq!(statuses[1]["exists"], true);
    assert_eq!(statuses[1]["updatedAt"], "2026-10-05T00:00:00.000Z");
}

#[tokio::test]
async fn install_package_reports_the_platform_failure_verbatim() {
    let (_dir, schema, _prefs) = fixture(json!(["QUERY_ALL_PACKAGES"]));
    let host = schema_host(&schema);
    stub(host.clone(), |method, _| match method {
        "systemInstallPackage" => {
            json!({"packageName":"com.a","lastUpdateTime": Value::Null,"isNew": true})
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { installPackage(path:"/tmp/a.apk") { id updatedAt isNew } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let pending = response.data.into_json().unwrap()["installPackage"].clone();
    assert_eq!(pending["id"], "com.a");
    assert_eq!(pending["isNew"], true);
    assert_eq!(pending["updatedAt"], Value::Null);
}

/// The schema owns the host handle; this reaches it without threading it
/// through every fixture.
fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}
