use super::*;
#[tokio::test]
async fn the_platform_owns_classification_and_its_answer_is_passed_through() {
    let host = std::sync::Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let adapter = host.clone();
    let task = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            assert_eq!(request["method"], "systemPhoneMetadata");
            // The number travels as typed: the platform parses it against
            // the region it reports, we never rewrite it here.
            let known = request["params"]["number"] == "18005551234"
                && request["params"]["region"] == "US";
            let result = if known {
                json!({"country":"US","numberType":"TOLL_FREE","carrier":"","description":"region"})
            } else {
                json!({"country":"","numberType":"","carrier":"","description":""})
            };
            let _ = adapter.reply(generation, json!({"id":request["id"],"result":result}));
        }
    });
    let runtime = Runtime::default();
    let us = LocaleFacts { region: "US".into(), locale: "en_US".into(), available: true };
    let geo = runtime.lookup(&host, "18005551234", &us).await.unwrap();
    assert_eq!(geo.country, "US");
    assert_eq!(geo.number_type, "TOLL_FREE");
    assert_eq!(geo.carrier, "");
    assert_eq!(geo.description, "region");
    // An unknown number comes back as nothing rather than an empty geo.
    for number in ["12345", "not-a-number", ""] {
        assert!(runtime.lookup(&host, number, &us).await.is_none());
    }
    // A platform without the data (iOS) is never asked.
    let ios = LocaleFacts { region: String::new(), locale: String::new(), available: false };
    assert!(runtime.lookup(&host, "18005551234", &ios).await.is_none());
    // Same number, different region: the cache is keyed by region too.
    let cn = LocaleFacts { region: "CN".into(), locale: "zh_CN".into(), available: true };
    assert!(runtime.lookup(&host, "18005551234", &cn).await.is_none());
    assert_eq!(runtime.lookup(&host, "18005551234", &us).await.unwrap().country, "US");
    task.abort();
}
