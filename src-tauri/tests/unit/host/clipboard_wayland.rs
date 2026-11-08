use super::*;
use rustix::net::{
    RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags, recvmsg, sendmsg,
};
use std::{
    collections::VecDeque,
    fs::File,
    io::{IoSlice, IoSliceMut, Write},
    mem::MaybeUninit,
};

const EXT: &str = "ext_data_control_manager_v1";
const WLR: &str = "zwlr_data_control_manager_v1";
const OFFER: u32 = 0xff00_0000;
const PRIMARY_OFFER: u32 = OFFER + 1;

struct Compositor {
    socket: UnixStream,
    descriptors: VecDeque<OwnedFd>,
}

struct Message {
    object: u32,
    opcode: u16,
    body: Vec<u8>,
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn string(bytes: &[u8], offset: usize) -> &str {
    let length = word(bytes, offset) as usize;
    std::str::from_utf8(&bytes[offset + 4..offset + 4 + length - 1]).unwrap()
}

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

fn wire_string(value: &str) -> Vec<u8> {
    let mut bytes = words(&[(value.len() + 1) as u32]);
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes
}

impl Compositor {
    fn pair() -> (Connection, Self) {
        let (client, socket) = UnixStream::pair().unwrap();
        socket.set_read_timeout(Some(TIMEOUT)).unwrap();
        let connection = Connection::from_socket(client).unwrap();
        (
            connection,
            Self {
                socket,
                descriptors: VecDeque::new(),
            },
        )
    }

    fn read(&mut self, mut bytes: &mut [u8]) {
        while !bytes.is_empty() {
            let mut buffer = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(4))];
            let mut control = RecvAncillaryBuffer::new(&mut buffer);
            let message = recvmsg(
                &self.socket,
                &mut [IoSliceMut::new(bytes)],
                &mut control,
                RecvFlags::empty(),
            )
            .unwrap();
            assert!(message.bytes > 0, "the clipboard client disconnected");
            for received in control.drain() {
                if let RecvAncillaryMessage::ScmRights(fds) = received {
                    self.descriptors.extend(fds);
                }
            }
            bytes = &mut bytes[message.bytes..];
        }
    }

    fn next(&mut self) -> Message {
        let mut header = [0; 8];
        self.read(&mut header);
        let opcode_and_size = word(&header, 4);
        let mut body = vec![0; (opcode_and_size >> 16) as usize - 8];
        self.read(&mut body);
        Message {
            object: word(&header, 0),
            opcode: opcode_and_size as u16,
            body,
        }
    }

    fn request(&mut self, object: u32, opcode: u16) -> Vec<u8> {
        let request = self.next();
        assert_eq!((request.object, request.opcode), (object, opcode));
        request.body
    }

    fn emit(&mut self, object: u32, opcode: u16, body: &[u8], fd: Option<&impl AsFd>) {
        let mut message = words(&[object, ((body.len() as u32 + 8) << 16) | u32::from(opcode)]);
        message.extend_from_slice(body);
        if let Some(fd) = fd {
            let mut buffer = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
            let mut control = SendAncillaryBuffer::new(&mut buffer);
            let descriptors = [fd.as_fd()];
            assert!(control.push(SendAncillaryMessage::ScmRights(&descriptors)));
            let count = sendmsg(
                &self.socket,
                &[IoSlice::new(&message)],
                &mut control,
                SendFlags::empty(),
            )
            .unwrap();
            self.socket.write_all(&message[count..]).unwrap();
        } else {
            self.socket.write_all(&message).unwrap();
        }
    }

    fn event(&mut self, object: u32, opcode: u16, body: &[u8]) {
        self.emit(object, opcode, body, None::<&UnixStream>);
    }

    fn sync(&mut self) {
        let callback = word(&self.request(1, 0), 0);
        self.event(callback, 0, &words(&[0]));
        self.event(1, 1, &words(&[callback]));
    }

    fn wait_for_disconnect(&mut self) {
        assert_eq!(self.socket.read(&mut [0]).unwrap(), 0);
    }

    fn registry(&mut self, managers: &[(&str, u32)]) -> u32 {
        let registry = word(&self.request(1, 1), 0);
        for (index, (interface, version)) in std::iter::once(("wl_seat", 1))
            .chain(managers.iter().copied())
            .enumerate()
        {
            let mut global = words(&[index as u32 + 10]);
            global.extend(wire_string(interface));
            global.extend(words(&[version]));
            self.event(registry, 0, &global);
        }
        self.sync();
        registry
    }

    fn device(&mut self, managers: &[(&str, u32)], expected: (&str, u32)) -> (u32, u32) {
        let registry = self.registry(managers);
        let bind = self.request(registry, 0);
        assert_eq!(string(&bind, 4), expected.0);
        assert_eq!(word(&bind, bind.len() - 8), expected.1);
        let manager = word(&bind, bind.len() - 4);
        let bind = self.request(registry, 0);
        assert_eq!(string(&bind, 4), "wl_seat");
        let seat = word(&bind, bind.len() - 4);
        let get_device = self.request(manager, 1);
        assert_eq!(word(&get_device, 4), seat);
        (manager, word(&get_device, 0))
    }
}

#[test]
fn reads_regular_unicode_clipboard_through_both_protocols_without_focus() {
    let text = "Clipboard fixture 文档 😀";
    for manager in [(EXT, 1), (WLR, 1), (WLR, 2)] {
        let (connection, mut compositor) = Compositor::pair();
        let server = thread::spawn(move || {
            let (_, device) = compositor.device(&[manager], manager);
            compositor.event(device, 0, &words(&[OFFER]));
            for mime in ["image/png", "STRING", "UTF8_STRING", UTF8_MIME] {
                compositor.event(OFFER, 0, &wire_string(mime));
            }
            compositor.event(device, 1, &words(&[OFFER]));
            if manager == (EXT, 1) || manager == (WLR, 2) {
                compositor.event(device, 0, &words(&[PRIMARY_OFFER]));
                compositor.event(PRIMARY_OFFER, 0, &wire_string(UTF8_MIME));
                compositor.event(device, 3, &words(&[PRIMARY_OFFER]));
            }
            compositor.sync();
            if manager == (EXT, 1) || manager == (WLR, 2) {
                compositor.request(PRIMARY_OFFER, 1);
            }
            let receive = compositor.request(OFFER, 0);
            assert_eq!(string(&receive, 0), UTF8_MIME);
            let mut pipe = File::from(compositor.descriptors.pop_front().unwrap());
            pipe.write_all(text.as_bytes()).unwrap();
            drop(pipe);
            compositor.sync();
            compositor.wait_for_disconnect();
        });
        assert_eq!(
            Session::from_connection(connection)
                .unwrap()
                .read_text()
                .unwrap()
                .as_deref(),
            Some(text)
        );
        server.join().unwrap();
    }
}

#[test]
fn both_protocols_serve_text_and_png_after_copy_returns_and_handle_source_cancel() {
    for (manager, image) in [
        ((EXT, 1), true),
        ((WLR, 1), true),
        ((EXT, 1), false),
        ((WLR, 2), false),
    ] {
        let (connection, mut compositor) = Compositor::pair();
        let (content, expected, types, requested) = if image {
            let png: Vec<u8> = (0..600_000).map(|index| (index % 251) as u8).collect();
            (Content::Png(png.clone()), png, IMAGE_TYPES, "image/png")
        } else {
            let text = "Clipboard fixture 文档 😀";
            (
                Content::Text(text.into()),
                text.as_bytes().to_vec(),
                TEXT_TYPES,
                "UTF8_STRING",
            )
        };
        let (pasted, result) = mpsc::sync_channel(1);
        let (paste, start_paste) = mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            let (global, device) = if manager.0 == EXT {
                compositor.device(&[(WLR, 2), manager], manager)
            } else {
                compositor.device(&[manager], manager)
            };
            compositor.sync();
            let source = word(&compositor.request(global, 0), 0);
            for mime in types {
                assert_eq!(string(&compositor.request(source, 0), 0), *mime);
            }
            assert_eq!(word(&compositor.request(device, 0), 0), source);
            compositor.sync();
            start_paste.recv_timeout(TIMEOUT).unwrap();

            let (mut reader, writer) = UnixStream::pair().unwrap();
            reader.set_read_timeout(Some(TIMEOUT)).unwrap();
            compositor.emit(source, 0, &wire_string(requested), Some(&writer));
            drop(writer);
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            if manager != (WLR, 2) {
                compositor.event(source, 1, &[]);
                compositor.request(source, 1);
            }
            pasted.send(bytes).unwrap();
            compositor.wait_for_disconnect();
        });
        let mut clipboard = Clipboard { publisher: None };
        clipboard
            .publish(Session::from_connection(connection).unwrap(), content)
            .unwrap();
        paste.send(()).unwrap();
        assert_eq!(result.recv_timeout(TIMEOUT).unwrap(), expected);
        drop(clipboard);
        server.join().unwrap();
    }
}

#[test]
fn compositor_without_data_control_is_rejected_for_xwayland_fallback() {
    let (connection, mut compositor) = Compositor::pair();
    let server = thread::spawn(move || {
        compositor.registry(&[]);
        compositor.wait_for_disconnect();
    });
    let error = Session::from_connection(connection).err().unwrap();
    assert!(
        error
            .to_string()
            .contains("does not advertise clipboard data control")
    );
    server.join().unwrap();
}
