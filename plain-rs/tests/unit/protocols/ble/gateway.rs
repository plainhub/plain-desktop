use super::*;
#[test]
fn auth_keeps_only_used_identity_and_digest() {
    let request = GatewayRequest::Auth {
        client_id: "client".into(),
        client_name: "手机".into(),
        password: "a".repeat(128),
    };
    let bytes = request.encode().unwrap();
    assert_eq!(GatewayRequest::decode(&bytes).unwrap(), request);
    assert_eq!(
        parts(&bytes).unwrap().0,
        &[
            3, 6, 0, b'c', b'l', b'i', b'e', b'n', b't', 6, 0, 0xe6, 0x89, 0x8b, 0xe6, 0x9c, 0xba
        ]
    );
}
#[test]
fn graphql_roundtrips_encrypted_bytes_and_rejects_extra_metadata() {
    let body = (0..8192).map(|n| n as u8).collect::<Vec<_>>();
    let request = GatewayRequest::Graphql {
        client_id: "client".into(),
        body,
    };
    let bytes = request.encode().unwrap();
    assert_eq!(GatewayRequest::decode(&bytes).unwrap(), request);
    let (metadata, body) = parts(&bytes).unwrap();
    let mut extra = metadata.to_vec();
    extra.push(0);
    assert!(GatewayRequest::decode(&message(&extra, body).unwrap()).is_err());
}
#[test]
fn invalid_digest_identity_and_unknown_operation_are_rejected() {
    for password in ["".into(), "g".repeat(128), "A".repeat(128)] {
        assert!(
            GatewayRequest::Auth {
                client_id: "id".into(),
                client_name: "".into(),
                password
            }
            .encode()
            .is_err()
        );
    }
    assert!(
        GatewayRequest::Graphql {
            client_id: "".into(),
            body: vec![]
        }
        .encode()
        .is_err()
    );
    assert!(GatewayRequest::decode(&message(&[9, 1, 0, b'A'], &[]).unwrap()).is_err());
}
