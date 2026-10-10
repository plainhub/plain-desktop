use super::*;
#[tokio::test]
async fn search_responses_reach_the_ephemeral_source_port() {
    let renderer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let control = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let request = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nST: ssdp:all\r\n\r\n";
    respond_to_search(
        &renderer,
        request,
        control.local_addr().unwrap(),
        "uuid:test",
        "192.0.2.1",
        7878,
    )
    .await;
    let mut buffer = [0; 2048];
    for _ in 0..3 {
        let (n, source) =
            tokio::time::timeout(Duration::from_secs(1), control.recv_from(&mut buffer))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(source, renderer.local_addr().unwrap());
        let packet = std::str::from_utf8(&buffer[..n]).unwrap();
        assert!(packet.starts_with("HTTP/1.1 200 OK"));
        assert!(packet.contains("USN: uuid:test"));
        assert!(packet.contains("LOCATION: http://192.0.2.1:7878/description.xml"));
    }
    respond_to_search(
        &renderer,
        "NOTIFY * HTTP/1.1",
        control.local_addr().unwrap(),
        "uuid:test",
        "192.0.2.1",
        7878,
    )
    .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), control.recv_from(&mut buffer))
            .await
            .is_err()
    );
}
#[test]
fn accepting_a_queued_play_applies_the_same_renderer_commands() {
    let mut state = DlnaRendererState::default();
    let pending = PendingCastRequest {
        sender_ip: "192.0.2.2".into(),
        sender_name: "Phone".into(),
        media_uri: "http://host/a.mp4".into(),
        media_title: "A".into(),
        media_type: crate::dlna_receiver::types::DlnaMediaType::VIDEO,
        album_art_uri: "http://host/art.jpg".into(),
    };
    state.pending_cast_request = Some(pending.clone());
    state.pending_play_queued = true;
    accept_pending(&mut state, &pending, true);
    assert!(state.pending_cast_request.is_none());
    assert_eq!(state.media_uri, pending.media_uri);
    assert_eq!(state.media_album_art_uri, pending.album_art_uri);
    assert_eq!(state.playback_state, DlnaPlaybackState::Playing);
    apply_command(&mut state, DlnaCommand::Pause);
    assert_eq!(state.playback_state, DlnaPlaybackState::PausedPlayback);
    apply_command(&mut state, DlnaCommand::Stop);
    assert!(state.media_uri.is_empty());
    assert_eq!(state.seek_target_ms, Some(0));
}
