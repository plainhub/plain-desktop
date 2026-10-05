use super::*;
#[test]
fn guest_wire_and_file_ids_use_existing_xchacha_contract_and_exact_query_encoding() {
    let link = Link::new("::1", 8443, "share +/?#中文", &URL_SAFE.encode([7; 32])).unwrap();
    assert!(link.page_url.starts_with("https://[::1]:8443/s/share"));
    let first = link.request(Some("folder/quoted\" 中文%")).unwrap();
    let second = link.request(None).unwrap();
    assert_ne!(&first[..24], &second[..24]);
    let plain = String::from_utf8(crate::xchacha_decrypt_raw(&[7; 32], &first).unwrap()).unwrap();
    let parts: Vec<_> = plain.splitn(3, '|').collect();
    assert!(
        parts[0]
            .parse::<u64>()
            .unwrap()
            .abs_diff(crate::chat::pairing::now_ms() as u64)
            < 1000
    );
    assert!(uuid::Uuid::parse_str(parts[1]).is_ok());
    let document: Value = serde_json::from_str(parts[2]).unwrap();
    assert_eq!(
        document["variables"]["virtualPath"],
        "folder/quoted\" 中文%"
    );
    let url = link
        .file_url(&STANDARD.encode([8; 32]), "folder/+/?#中文%", false)
        .unwrap();
    let url = reqwest::Url::parse(&url).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(query["sid"], link.shared_id);
    let file: Value = serde_json::from_slice(
        &crate::xchacha_decrypt_raw(&[8; 32], &crate::base64_decode(&query["id"])).unwrap(),
    )
    .unwrap();
    assert_eq!(
        file,
        json!({"sharedId":link.shared_id,"virtualPath":"folder/+/?#中文%"})
    );
    assert!(link.file_url("bad", "", false).is_err());
    for host in ["localhost/path", "evil@localhost", "a,b", "a?b", "a\\b"] {
        assert!(Link::new(host, 443, "id", &URL_SAFE.encode([7; 32])).is_err());
    }
}
#[test]
fn guest_response_never_accepts_unauthenticated_data_or_invalid_file_tokens() {
    let link = Link::new("localhost", 443, "id", &URL_SAFE.encode([7; 32])).unwrap();
    let plain=json!({"data":{"sharedInfo":{"name":"share","readOnly":true,"requiresPassword":false,"expiresAt":null,"urlToken":STANDARD.encode([8;32]),"entries":[]}}}).to_string();
    assert!(link.response(plain.as_bytes()).is_err());
    assert_eq!(
        link.response(&crate::xchacha_encrypt_raw(&[7; 32], plain.as_bytes()).unwrap())
            .unwrap()
            .name,
        "share"
    );
    assert!(
        link.response(&crate::xchacha_encrypt_raw(&[9; 32], plain.as_bytes()).unwrap())
            .is_err()
    );
    let errors =
        crate::xchacha_encrypt_raw(&[7; 32], br#"{"errors":[{"message":"expired"}]}"#).unwrap();
    assert_eq!(link.response(&errors).unwrap_err().to_string(), "expired");
}
