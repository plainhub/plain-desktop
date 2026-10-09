use super::*;
#[test]
fn encoder_retains_message_and_roundtrips_native_handle() {
    let body = (0..8192).map(|v| v as u8).collect::<Vec<_>>();
    let message = call(0, 0, 0, 0, 0, false, &body).unwrap().unwrap();
    let encode = encoder(17, true, 512, &message).unwrap();
    let assembly = plain_ble_new();
    let mut complete = None;
    while let Some(frame) = call(4, encode, 0, 0, 0, false, &[]).unwrap() {
        assert!(frame.len() <= 512);
        if let Some(data) = call(3, assembly, 0, 0, 0, false, &frame).unwrap() {
            complete = Some(data);
        }
    }
    assert_eq!(plain_ble_info(assembly), 17 | (1 << 32));
    assert_eq!(
        call(1, 0, 0, 0, 0, false, &complete.unwrap())
            .unwrap()
            .unwrap(),
        body
    );
    plain_ble_free(encode);
    plain_ble_free(assembly);
    assert!(call(4, encode, 0, 0, 0, false, &[]).is_err());
    assert!(call(3, assembly, 0, 0, 0, false, &[]).is_err());
}
#[test]
fn c_buffer_reports_error_and_is_released() {
    let buffer = unsafe { plain_ble_call(99, 0, 0, 0, 0, false, std::ptr::null(), 0) };
    assert_eq!(buffer.status, -1);
    assert!(buffer.len > 0);
    unsafe {
        plain_ble_buffer_free(buffer);
    }
}

#[test]
fn gateway_client_and_server_share_binary_crypto_contract() {
    let key = [7u8; 32];
    let token = crate::utils::base64::base64_encode(&key);
    let args = serde_json::json!({"clientId":"peer", "token":token, "body":"{\"query\":\"{ __typename }\"}"});
    let bytes = call(6, 0, 0, 0, 0, false, &serde_json::to_vec(&args).unwrap())
        .unwrap()
        .unwrap();
    let ble_wire::GatewayRequest::Graphql { client_id, body } =
        ble_wire::GatewayRequest::decode(&bytes).unwrap()
    else {
        panic!("wrong operation")
    };
    assert_eq!(client_id, "peer");
    assert_eq!(
        crate::crypto::xchacha_decrypt_raw(&key, &body).unwrap(),
        b"{\"query\":\"{ __typename }\"}"
    );
    let encrypted = crate::crypto::xchacha_encrypt_raw(&key, b"{\"data\":{}}").unwrap();
    let mut response = key.to_vec();
    response.extend(ble_wire::response(200, &encrypted).unwrap());
    assert_eq!(
        call(8, 0, 0, 0, 0, false, &response).unwrap().unwrap(),
        b"{\"data\":{}}"
    );
    response[0] ^= 1;
    assert!(call(8, 0, 0, 0, 0, false, &response).is_err());
}

#[test]
fn gateway_auth_hashes_password_in_shared_core() {
    let args = br#"{"clientId":"peer","clientName":"phone","password":"abc"}"#;
    let bytes = call(5, 0, 0, 0, 0, false, args).unwrap().unwrap();
    let ble_wire::GatewayRequest::Auth {
        client_id,
        client_name,
        password,
    } = ble_wire::GatewayRequest::decode(&bytes).unwrap()
    else {
        panic!("wrong operation")
    };
    assert_eq!(client_id, "peer");
    assert_eq!(client_name, "phone");
    assert_eq!(
        password,
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    assert!(
        call(
            5,
            0,
            0,
            0,
            0,
            false,
            br#"{"clientId":"peer","clientName":"phone","password":"abc","unused":true}"#
        )
        .is_err()
    );
    assert!(
        call(
            7,
            0,
            0,
            0,
            0,
            false,
            &ble_wire::response(401, b"denied").unwrap()
        )
        .is_err()
    );
}
