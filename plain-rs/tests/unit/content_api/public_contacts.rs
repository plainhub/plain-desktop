use super::*;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use serde_json::{Value, json};

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

fn fixture(permissions: Value) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("api_permissions", permissions).unwrap();
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs, db, directory),
    )
}

fn contact_fact(id: &str) -> Value {
    json!({
        "id": id, "prefix": "Dr", "firstName": "Ada", "middleName": "M", "lastName": "Lovelace",
        "suffix": "Jr", "nickname": "ada", "photoId": "content://photo/1", "source": "Local",
        "starred": true, "contactId": "c1", "thumbnailId": "content://thumb/1", "notes": "note",
        "ringtone": "content://ring/1", "updatedAt": "2026-10-05T00:00:00Z",
        "phoneNumbers": [
            {"value": "+1 555", "type": 2, "label": "mobile", "normalizedNumber": "+1555"},
            {"value": "+1 999", "type": 99, "label": "", "normalizedNumber": "+1999"},
        ],
        "emails": [{"value": "ada@example.com", "type": 1, "label": "home"}],
        "addresses": [{"value": "1 Main St", "type": 1, "label": ""}],
        "events": [{"value": "1815-12-10", "type": 2, "label": ""}],
        "websites": [{"value": "https://example.com", "type": 4, "label": ""}],
        "ims": [{"value": "ada@example.im", "type": 6, "label": "custom"}],
        "groups": [{"id": "7", "name": "Friends"}],
        "organization": {"company": "Analytical", "title": "Engineer"},
    })
}

fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}

#[test]
fn android_data_codes_map_to_the_contract_enums() {
    assert_eq!(PhoneType::from_android(0), PhoneType::Custom);
    assert_eq!(PhoneType::from_android(2), PhoneType::Mobile);
    assert_eq!(PhoneType::from_android(19), PhoneType::Assistant);
    // Unknown codes fall back to CUSTOM so the label keeps carrying the value.
    assert_eq!(PhoneType::from_android(99), PhoneType::Custom);
    assert_eq!(EmailType::from_android(4), EmailType::Mobile);
    assert_eq!(EventType::from_android(2), EventType::Birthday);
    assert_eq!(ImProtocol::from_android(6), ImProtocol::GoogleTalk);
    // Every member must round-trip: the write path sends the code back.
    for kind in 0..=19 {
        assert_eq!(PhoneType::from_android(kind).android(), kind);
    }
}

/// The write path sends the enum *name* and the platform deserializes by
/// name, so it has to be the name the schema publishes. Renaming a variant
/// without updating this would break every contact write with a detail row.
#[test]
fn detail_enum_names_are_exactly_the_members_the_schema_publishes() {
    let (_dir, schema) = fixture(json!([]));
    let sdl = schema.sdl();
    let members = |name: &str| {
        let body = sdl
            .split_once(&format!("enum {name} {{"))
            .expect("enum in schema")
            .1
            .split_once('}')
            .expect("closed enum")
            .0;
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('"'))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let all = |codes: std::ops::RangeInclusive<i64>, name: fn(i64) -> String| {
        codes.map(name).collect::<Vec<_>>()
    };
    assert_eq!(
        members("PhoneType"),
        all(0..=19, |code| PhoneType::from_android(code).name())
    );
    assert_eq!(
        members("EmailType"),
        all(0..=4, |code| EmailType::from_android(code).name())
    );
    assert_eq!(
        members("PostalType"),
        all(0..=3, |code| PostalType::from_android(code).name())
    );
    assert_eq!(
        members("EventType"),
        all(0..=3, |code| EventType::from_android(code).name())
    );
    assert_eq!(
        members("WebsiteType"),
        all(0..=6, |code| WebsiteType::from_android(code).name())
    );
    assert_eq!(
        members("ImProtocol"),
        all(0..=9, |code| ImProtocol::from_android(code).name())
    );
}

#[tokio::test]
async fn contacts_map_the_host_facts_onto_the_contract_type() {
    let (_dir, schema) = fixture(json!(["READ_CONTACTS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemContactFacts" => {
            assert_eq!(params["query"], json!("text:ada"));
            assert_eq!(params["offset"], 0);
            assert_eq!(params["limit"], 20);
            json!([contact_fact("raw-1")])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { contacts(offset:0, limit:20, query:"text:ada") {
                 id prefix firstName middleName lastName suffix nickname photoId source starred
                 contactId thumbnailId notes ringtone updatedAt
                 phoneNumbers { value type label normalizedNumber }
                 emails { value type label } addresses { value type label }
                 events { value type label } websites { value type label }
                 ims { value protocol customProtocol }
                 groups { id name contactCount }
                 organization { company title }
                 tags { id name count } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let contact = &response.data.into_json().unwrap()["contacts"][0];
    assert_eq!(contact["id"], "raw-1");
    assert_eq!(contact["firstName"], "Ada");
    assert_eq!(contact["lastName"], "Lovelace");
    assert_eq!(contact["starred"], true);
    assert_eq!(contact["phoneNumbers"][0]["type"], "MOBILE");
    assert_eq!(contact["phoneNumbers"][1]["type"], "CUSTOM");
    assert_eq!(contact["emails"][0]["type"], "HOME");
    assert_eq!(contact["events"][0]["type"], "BIRTHDAY");
    assert_eq!(contact["websites"][0]["type"], "HOME");
    assert_eq!(contact["ims"][0]["protocol"], "GOOGLE_TALK");
    assert_eq!(contact["ims"][0]["customProtocol"], "custom");
    assert_eq!(contact["groups"][0]["id"], "7");
    assert_eq!(contact["organization"]["company"], "Analytical");
    assert_eq!(contact["updatedAt"], "2026-10-05T00:00:00.000Z");
    assert_eq!(contact["tags"], json!([]));
}

#[tokio::test]
async fn contact_reads_refuse_to_answer_without_the_web_permission() {
    let (_dir, schema) = fixture(json!([]));
    for query in [
        r#"query { contacts(offset:0, limit:1, query:"") { id } }"#,
        r#"query { contactGroups { id } }"#,
        r#"query { contactSources { name } }"#,
    ] {
        let response = schema.execute(query).await;
        assert_eq!(
            response.errors[0].message, "no_permission",
            "ungated contact root: {query}"
        );
    }
    // contactCount degrades to 0 instead of erroring, like plain-app.
    let response = schema.execute(r#"query { contactCount(query:"") }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["contactCount"], 0);
}

#[tokio::test]
async fn create_contact_reads_the_new_row_back_through_the_provider_search() {
    let (_dir, schema) = fixture(json!(["WRITE_CONTACTS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemCreateContact" => {
            // The payload the platform deserializes into its ContactInput.
            assert_eq!(params["input"]["firstName"], "Ada");
            assert_eq!(params["input"]["phoneNumbers"][0]["type"], "MOBILE");
            assert_eq!(params["input"]["groupIds"], json!(["7"]));
            Value::from("raw-9")
        }
        "systemContactFacts" => {
            assert_eq!(params["query"], "id=raw-9");
            json!([contact_fact("raw-9")])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { createContact(input: {
                 prefix: "", firstName: "Ada", middleName: "", lastName: "Lovelace", suffix: "",
                 nickname: "", phoneNumbers: [{value:"+1 555", type:MOBILE, label:"mobile"}],
                 emails: [], addresses: [], events: [], source: "Local", starred: false,
                 notes: "", groupIds: ["7"], websites: [], ims: [] }) { id firstName starred } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let created = &response.data.into_json().unwrap()["createContact"];
    assert_eq!(created["id"], "raw-9");
    assert_eq!(created["firstName"], "Ada");
}

#[tokio::test]
async fn a_write_that_cannot_be_read_back_fails_the_mutation() {
    let (_dir, schema) = fixture(json!(["WRITE_CONTACTS"]));
    stub(schema_host(&schema), |method, _| match method {
        "systemUpdateContact" => json!(true),
        "systemContactFacts" => json!([]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { updateContact(id:"gone", input: {
                 prefix: "", firstName: "Ada", middleName: "", lastName: "", suffix: "",
                 nickname: "", phoneNumbers: [], emails: [], addresses: [], events: [],
                 source: "Local", starred: false, notes: "", groupIds: [], websites: [], ims: [] })
                 { id } }"#,
        )
        .await;
    assert_eq!(
        response.errors[0].message,
        "Contact gone not found after update"
    );
}

#[tokio::test]
async fn delete_contacts_counts_only_the_ids_the_platform_deleted() {
    let (_dir, schema) = fixture(json!(["WRITE_CONTACTS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemContactIds" => {
            assert_eq!(params["query"], "all:true");
            json!(["a", "b", "c"])
        }
        "systemDeleteRecords" => {
            assert_eq!(params["provider"], "CONTACT");
            assert_eq!(params["ids"], json!(["a", "b", "c"]));
            json!(["a", "c"])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteContacts(query:"all:true") { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteContacts"]["affectedCount"],
        2
    );
}

#[tokio::test]
async fn delete_contacts_rejects_a_blank_query_before_touching_the_platform() {
    let (_dir, schema) = fixture(json!(["WRITE_CONTACTS"]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    let response = schema
        .execute(r#"mutation { deleteContacts(query:"  ") { affectedCount } }"#)
        .await;
    assert!(
        response.errors[0]
            .message
            .starts_with("query is required for bulk mutations"),
        "{:?}",
        response.errors
    );
}

#[tokio::test]
async fn group_writes_return_the_group_they_just_wrote() {
    let (_dir, schema) = fixture(json!(["WRITE_CONTACTS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemCreateContactGroup" => {
            assert_eq!(params["name"], "Friends");
            assert_eq!(params["accountName"], "Local");
            Value::from("42")
        }
        "systemUpdateContactGroup" => {
            assert_eq!(params["id"], "42");
            json!(true)
        }
        "systemDeleteContactGroup" => json!(true),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { createContactGroup(name:"Friends", accountName:"Local", accountType:"LOCAL")
                 { id name contactCount } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let group = &response.data.into_json().unwrap()["createContactGroup"];
    assert_eq!(group["id"], "42");
    assert_eq!(group["name"], "Friends");

    let response = schema
        .execute(r#"mutation { updateContactGroup(id:"42", name:"Close") { id name } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let group = &response.data.into_json().unwrap()["updateContactGroup"];
    assert_eq!(group["name"], "Close");

    let response = schema
        .execute(r#"mutation { deleteContactGroup(id:"42") }"#)
        .await;
    assert_eq!(
        response.data.into_json().unwrap()["deleteContactGroup"],
        true
    );
}
