use super::*;

#[test]
fn fixed_peer_wire_vector() {
    let request = Request::PeerGraphql {
        client_id: "A".into(),
        channel_id: "".into(),
        body: vec![0, 255],
    };
    let data = request.encode().unwrap();
    assert_eq!(data, [6, 0, 0, 0, 2, 0, 0, 0, 1, 1, 0, 65, 0, 0, 0, 255]);
    assert_eq!(Request::decode(&data).unwrap(), request);
    assert_eq!(
        frame(&data, 0x12345678, true, 0, 512).unwrap().unwrap(),
        [vec![1, 7, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0], data].concat()
    );
}
#[test]
fn assembler_roundtrips_all_bytes_across_mtu_sizes() {
    let data = message(&[], &(0..8192).map(|i| i as u8).collect::<Vec<_>>()).unwrap();
    for limit in [20, 182, 244, 512] {
        let mut assembler = Assembler::default();
        let mut sequence = 0;
        let mut complete = None;
        while let Some(frame) = frame(&data, 7, false, sequence, limit).unwrap() {
            assert!(frame.len() <= limit);
            let result = assembler.push(&frame).unwrap();
            if result.is_some() {
                assert!(complete.is_none());
                complete = result;
            }
            sequence += 1;
        }
        assert_eq!(complete.unwrap(), data);
        assert_eq!(assembler.info(), 7);
    }
}
#[test]
fn utf8_and_empty_nearby() {
    let data = Request::PeerGraphql {
        client_id: "身份😀".into(),
        channel_id: "频道".into(),
        body: vec![],
    }
    .encode()
    .unwrap();
    assert_eq!(Request::decode(&data).unwrap().encode().unwrap(), data);
    let empty = message(&[], &[]).unwrap();
    assert_eq!(nearby_body(&empty).unwrap(), b"");
    assert!(nearby_body(&data).is_err());
}
#[test]
fn file_range_checks_and_exact_decode() {
    let request = Request::FileChunk {
        client_id: "actor".into(),
        file_id: "encrypted+/=".into(),
        offset: 1u64 << 40,
        length: 8192,
    };
    let mut data = request.encode().unwrap();
    assert_eq!(Request::decode(&data).unwrap(), request);
    data.push(0);
    assert!(Request::decode(&data).is_err());
    for (offset, length) in [(0, 0), (0, 8193), (u64::MAX, 1)] {
        assert!(
            Request::FileChunk {
                client_id: "a".into(),
                file_id: "f".into(),
                offset,
                length
            }
            .encode()
            .is_err()
        );
    }
}
#[test]
fn malformed_frames_reset_and_allow_clean_retry() {
    let data = message(&[], &[1; 40]).unwrap();
    let first = frame(&data, 1, false, 0, 20).unwrap().unwrap();
    let second = frame(&data, 1, false, 1, 20).unwrap().unwrap();
    let mut bads = vec![first.clone(), second.clone()];
    let mut version = second.clone();
    version[0] = 2;
    bads.push(version);
    let mut flags = second.clone();
    flags[1] |= 128;
    bads.push(flags);
    let mut id = second.clone();
    id[2] = 2;
    bads.push(id);
    let mut direction = second.clone();
    direction[1] |= 4;
    bads.push(direction);
    let mut skip = second.clone();
    skip[6] = 3;
    bads.push(skip);
    bads.push(vec![0; 9]);
    for bad in bads {
        let mut a = Assembler::default();
        a.push(&first).unwrap();
        if bad == second {
            a.push(&second).unwrap();
        }
        assert!(a.push(&bad).is_err());
        assert!(a.push(&first).unwrap().is_none());
    }
}
#[test]
fn missing_start_and_end_and_declared_limits() {
    let data = message(&[], &[1; 40]).unwrap();
    assert!(
        Assembler::default()
            .push(&frame(&data, 1, false, 1, 20).unwrap().unwrap())
            .is_err()
    );
    let mut full = frame(&data, 1, false, 0, 512).unwrap().unwrap();
    full[1] &= !2;
    assert!(Assembler::default().push(&full).is_err());
    let mut large = frame(&data, 1, false, 0, 512).unwrap().unwrap();
    large[14..18].copy_from_slice(&((MAX_BODY + 1) as u32).to_le_bytes());
    assert!(Assembler::default().push(&large).is_err());
    assert!(frame(&data, 0, false, 0, 512).is_err());
    assert!(frame(&data, 1, false, 0, 10).is_err());
}
#[test]
fn response_keeps_failure_status_and_raw_bytes() {
    let bytes = response(403, &[255, 0, 1]).unwrap();
    assert_eq!(decode_response(&bytes).unwrap(), (403, &[255, 0, 1][..]));
    assert!(response(99, &[]).is_err());
    assert!(response(600, &[]).is_err());
    assert!(decode_response(&message(&[], &[]).unwrap()).is_err());
}
#[test]
fn rejects_unknown_operation_invalid_utf8_and_trailing_metadata() {
    assert!(Request::decode(&message(&[9, 1, 0, b'A'], &[]).unwrap()).is_err());
    assert!(Request::decode(&message(&[1, 1, 0, 255, 0, 0], &[]).unwrap()).is_err());
    assert!(Request::decode(&message(&[1, 1, 0, b'A', 0, 0, 9], &[]).unwrap()).is_err());
}
#[test]
fn timeout_rejects_continuation() {
    let data = message(&[], &[1; 40]).unwrap();
    let mut a = Assembler::default();
    a.push(&frame(&data, 1, false, 0, 20).unwrap().unwrap())
        .unwrap();
    a.last = Some(Instant::now() - Duration::from_secs(16));
    assert!(
        a.push(&frame(&data, 1, false, 1, 20).unwrap().unwrap())
            .is_err()
    );
}
