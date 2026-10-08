use super::*;
#[tokio::test]
async fn offline_classification_preserves_region_and_toll_free_regressions() {
    let host = std::sync::Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let adapter=host.clone();
    let task=tokio::spawn(async move {
        while let Some(request)=requests.recv().await {
            assert_eq!(request["method"],"systemPhoneMetadata");
            let carrier=if request["params"]["includeCarrier"]==true { "carrier" } else { "" };
            let _ = adapter.reply(generation,json!({"id":request["id"],"result":{"carrier":carrier,"description":"region"}}));
        }
    });
    let runtime=Runtime::default();
    let us=LocaleFacts{region:"US".into(),locale:"en_US".into(),available:true};
    for number in ["18005551234","+18005551234"] {
        let geo=runtime.lookup(&host,number,&us).await.unwrap();
        assert_eq!(geo.country,"US");assert_eq!(geo.number_type,"TOLL_FREE");assert_eq!(geo.carrier,"");
    }
    let cn=LocaleFacts{region:"CN".into(),locale:"zh_CN".into(),available:true};
    let geo=runtime.lookup(&host,"18012345678",&cn).await.unwrap();
    assert_eq!(geo.country,"CN");assert_eq!(geo.number_type,"MOBILE");assert!(!geo.carrier.is_empty());
    assert!(runtime.lookup(&host,"18012345678",&us).await.is_none_or(|geo|geo.country!="CN"));
    for number in ["12345","not-a-number",""] {assert!(runtime.lookup(&host,number,&us).await.is_none());}
    task.abort();
}
