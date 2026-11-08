use super::*;

#[test]
fn incremental_copy_keeps_the_original_payload_and_terminates() {
    let original: Vec<u8> = (0..600_000).map(|index| (index % 251) as u8).collect();
    let mut current = Payload {
        acquired: 12,
        targets: vec![10],
        bytes: Arc::from(original.clone()),
    };
    let mut transfer = Transfer {
        payload: current.clone(),
        target: 10,
        offset: 0,
        deadline: Instant::now(),
    };
    let mut pasted = transfer.next(256 * 1024).to_vec();
    current.bytes = Arc::from([9u8, 8, 7]);
    pasted.extend_from_slice(transfer.next(256 * 1024));
    pasted.extend_from_slice(transfer.next(256 * 1024));
    assert!(transfer.next(256 * 1024).is_empty());
    assert_eq!(pasted, original);
    assert_eq!(&*current.bytes, &[9, 8, 7]);
}

#[test]
fn clipboard_text_decodes_utf8_and_legacy_latin1() {
    let text = "Fixture 文档 😀";
    assert_eq!(decode_text(1234, text.as_bytes().to_vec()).unwrap(), text);
    assert_eq!(
        decode_text(AtomEnum::STRING.into(), vec![b'c', b'a', b'f', 0xe9]).unwrap(),
        "café"
    );
    assert!(decode_text(1234, vec![0xff]).is_err());
}

#[test]
fn owner_serves_incremental_image_bytes_to_another_x11_client() {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
    };
    use x11rb::{
        protocol::xproto::{
            CHANGE_PROPERTY_REQUEST, CHANGE_WINDOW_ATTRIBUTES_REQUEST, GET_INPUT_FOCUS_REQUEST,
            GET_SELECTION_OWNER_REQUEST, GetInputFocusReply, GetSelectionOwnerReply,
            PROPERTY_NOTIFY_EVENT, PropertyNotifyEvent, SELECTION_REQUEST_EVENT,
            SEND_EVENT_REQUEST, SET_SELECTION_OWNER_REQUEST, Screen, Setup,
        },
        rust_connection::DefaultStream,
        x11_utils::Serialize,
    };

    const WINDOW: Window = 20;
    const PASTE_WINDOW: Window = 21;
    const PASTE_PROPERTY: Atom = 22;
    let atoms = Atoms {
        CLIPBOARD: 100,
        CLIPBOARD_MANAGER: 101,
        SAVE_TARGETS: 102,
        TARGETS: 103,
        TIMESTAMP: 104,
        MULTIPLE: 105,
        ATOM_PAIR: 106,
        INCR: 107,
        UTF8_STRING: 108,
        UTF8_MIME: 109,
        UTF8_MIME_UPPERCASE: 110,
        TEXT: 111,
        PLAIN_TEXT: 112,
        PNG: 113,
        PLAIN_CLIPBOARD_TRANSFER: 114,
        PLAIN_CLIPBOARD_WAKE: 115,
    };
    let (stream, mut server) = UnixStream::pair().unwrap();
    server.set_read_timeout(Some(TIMEOUT)).unwrap();
    let (pasted, result) = mpsc::sync_channel(1);
    let socket_atoms = atoms.clone();
    let server = thread::spawn(move || {
        let mut setup_request = [0; 12];
        server.read_exact(&mut setup_request).unwrap();
        let mut setup = Setup {
            status: 1,
            protocol_major_version: 11,
            resource_id_base: 0x0200_0000,
            resource_id_mask: 0x001f_ffff,
            maximum_request_length: 65535,
            roots: vec![Screen {
                root: 1,
                ..Screen::default()
            }],
            ..Setup::default()
        };
        setup.length = ((setup.serialize().len() - 8) / 4) as u16;
        server.write_all(&setup.serialize()).unwrap();

        let mut owner = NONE;
        let mut sequence = 0u16;
        let mut requested = false;
        let mut received = Vec::new();
        let value = |bytes: &[u8], offset| {
            u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
        };
        let packet = |bytes: &[u8]| {
            let mut packet = [0; 32];
            packet[..bytes.len()].copy_from_slice(bytes);
            packet
        };
        loop {
            let mut header = [0; 4];
            match server.read_exact(&mut header) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(error) => panic!("fixture X11 request failed: {error}"),
            }
            let mut request = vec![0; 4 * u16::from_ne_bytes([header[2], header[3]]) as usize];
            request[..4].copy_from_slice(&header);
            server.read_exact(&mut request[4..]).unwrap();
            sequence = sequence.wrapping_add(1);
            let property = |window, atom, state| {
                packet(
                    &PropertyNotifyEvent {
                        response_type: PROPERTY_NOTIFY_EVENT,
                        sequence,
                        window,
                        atom,
                        time: 42,
                        state,
                    }
                    .serialize(),
                )
            };
            match request[0] {
                GET_INPUT_FOCUS_REQUEST => {
                    let reply = GetInputFocusReply {
                        sequence,
                        ..Default::default()
                    }
                    .serialize();
                    server.write_all(&packet(&reply)).unwrap();
                }
                SET_SELECTION_OWNER_REQUEST => {
                    assert_eq!(value(&request, 12), 42);
                    owner = value(&request, 4);
                }
                GET_SELECTION_OWNER_REQUEST => {
                    let selection = value(&request, 4);
                    let reply = GetSelectionOwnerReply {
                        sequence,
                        length: 0,
                        owner: if selection == socket_atoms.CLIPBOARD {
                            owner
                        } else {
                            NONE
                        },
                    }
                    .serialize();
                    server.write_all(&packet(&reply)).unwrap();
                    if selection == socket_atoms.CLIPBOARD && owner == WINDOW && !requested {
                        requested = true;
                        let event = SelectionRequestEvent {
                            response_type: SELECTION_REQUEST_EVENT,
                            sequence,
                            time: 42,
                            owner,
                            requestor: PASTE_WINDOW,
                            selection,
                            target: socket_atoms.PNG,
                            property: PASTE_PROPERTY,
                        };
                        server.write_all(&packet(&event.serialize())).unwrap();
                    }
                }
                SEND_EVENT_REQUEST if value(&request, 4) == WINDOW => {
                    let mut event = request[12..44].to_vec();
                    event[0] |= 0x80;
                    event[2..4].copy_from_slice(&sequence.to_ne_bytes());
                    server.write_all(&event).unwrap();
                }
                SEND_EVENT_REQUEST => {
                    assert_eq!(value(&request, 4), PASTE_WINDOW);
                    assert_eq!(value(&request, 32), PASTE_PROPERTY);
                    server
                        .write_all(&property(PASTE_WINDOW, PASTE_PROPERTY, Property::DELETE))
                        .unwrap();
                }
                CHANGE_WINDOW_ATTRIBUTES_REQUEST => {}
                CHANGE_PROPERTY_REQUEST if value(&request, 4) == WINDOW => {
                    server
                        .write_all(&property(
                            WINDOW,
                            socket_atoms.PLAIN_CLIPBOARD_WAKE,
                            Property::NEW_VALUE,
                        ))
                        .unwrap();
                }
                CHANGE_PROPERTY_REQUEST => {
                    assert_eq!(value(&request, 4), PASTE_WINDOW);
                    assert_eq!(value(&request, 8), PASTE_PROPERTY);
                    let kind = value(&request, 12);
                    if kind == socket_atoms.INCR {
                        assert_eq!(value(&request, 24), 600);
                    } else {
                        assert_eq!(kind, socket_atoms.PNG);
                        let size = value(&request, 20) as usize;
                        if size == 0 {
                            pasted.send(std::mem::take(&mut received)).unwrap();
                        } else {
                            assert!(size <= 256);
                            received.extend_from_slice(&request[24..24 + size]);
                            server
                                .write_all(&property(
                                    PASTE_WINDOW,
                                    PASTE_PROPERTY,
                                    Property::DELETE,
                                ))
                                .unwrap();
                        }
                    }
                }
                opcode => panic!("unexpected fixture X11 opcode {opcode}"),
            }
        }
    });

    let (stream, _) = DefaultStream::from_unix_stream(stream).unwrap();
    let conn = Arc::new(RustConnection::connect_to_stream(stream, 0).unwrap());
    let wake = atoms.PLAIN_CLIPBOARD_WAKE;
    let selection = Selection {
        conn: conn.clone(),
        window: WINDOW,
        atoms,
    };
    let (commands, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        Owner {
            selection,
            current: None,
            transfers: HashMap::new(),
            chunk_size: 256,
        }
        .run(receiver)
        .unwrap();
    });
    let clipboard = Clipboard {
        conn,
        window: WINDOW,
        wake,
        commands,
        worker: Some(worker),
    };
    let original: Vec<u8> = (0..600).map(|index| (index % 251) as u8).collect();
    clipboard.write(Content::Png(original.clone())).unwrap();
    assert_eq!(result.recv_timeout(TIMEOUT).unwrap(), original);
    drop(clipboard);
    server.join().unwrap();
}
