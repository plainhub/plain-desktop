use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use x11rb::{
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE,
    connection::{Connection, RequestConnection},
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _,
            CreateWindowAux, EventMask, PropMode, Property, SELECTION_NOTIFY_EVENT,
            SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use super::{ClipboardResult, Content, MAX_TEXT_BYTES};

const TIMEOUT: Duration = Duration::from_secs(4);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_TRANSFERS: usize = 16;

x11rb::atom_manager! {
    Atoms: AtomCookies {
        CLIPBOARD,
        CLIPBOARD_MANAGER,
        SAVE_TARGETS,
        TARGETS,
        TIMESTAMP,
        MULTIPLE,
        ATOM_PAIR,
        INCR,
        UTF8_STRING,
        UTF8_MIME: b"text/plain;charset=utf-8",
        UTF8_MIME_UPPERCASE: b"text/plain;charset=UTF-8",
        TEXT,
        PLAIN_TEXT: b"text/plain",
        PNG: b"image/png",
        PLAIN_CLIPBOARD_TRANSFER,
        PLAIN_CLIPBOARD_WAKE,
    }
}

impl Atoms {
    fn utf8_targets(&self) -> [Atom; 3] {
        [self.UTF8_STRING, self.UTF8_MIME, self.UTF8_MIME_UPPERCASE]
    }
}

struct Selection {
    conn: Arc<RustConnection>,
    window: Window,
    atoms: Atoms,
}

impl Selection {
    fn connect() -> ClipboardResult<Self> {
        let (conn, screen) = x11rb::connect(None)?;
        let window = conn.generate_id()?;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            conn.setup().roots[screen].root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
        )?
        .check()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        Ok(Self {
            conn: Arc::new(conn),
            window,
            atoms,
        })
    }

    fn convert(&self, selection: Atom, target: Atom) -> ClipboardResult<()> {
        self.conn
            .convert_selection(
                self.window,
                selection,
                target,
                self.atoms.PLAIN_CLIPBOARD_TRANSFER,
                CURRENT_TIME,
            )?
            .check()?;
        Ok(())
    }

    fn event_until(
        &self,
        deadline: Instant,
        matches: impl Fn(&Event) -> bool,
    ) -> ClipboardResult<Event> {
        while Instant::now() < deadline {
            match self.conn.poll_for_event()? {
                Some(event) if matches(&event) => return Ok(event),
                Some(_) => {}
                None => thread::sleep(POLL_INTERVAL),
            }
        }
        Err("X11 clipboard transfer timed out".into())
    }

    fn read_text(&self) -> ClipboardResult<Option<String>> {
        let atoms = &self.atoms;
        if self
            .conn
            .get_selection_owner(atoms.CLIPBOARD)?
            .reply()?
            .owner
            == NONE
        {
            return Ok(None);
        }
        let [utf8, mime, uppercase] = atoms.utf8_targets();
        let deadline = Instant::now() + TIMEOUT;
        for target in [
            utf8,
            mime,
            uppercase,
            AtomEnum::STRING.into(),
            atoms.TEXT,
            atoms.PLAIN_TEXT,
        ] {
            self.convert(atoms.CLIPBOARD, target)?;
            let event = self.event_until(deadline, |event| {
                matches!(event, Event::SelectionNotify(event)
                    if event.requestor == self.window && event.selection == atoms.CLIPBOARD && event.target == target)
            })?;
            let Event::SelectionNotify(event) = event else {
                unreachable!()
            };
            if event.property == atoms.PLAIN_CLIPBOARD_TRANSFER {
                return self.read_property(deadline);
            }
        }
        Ok(None)
    }

    fn property(&self) -> ClipboardResult<x11rb::protocol::xproto::GetPropertyReply> {
        Ok(self
            .conn
            .get_property(
                true,
                self.window,
                self.atoms.PLAIN_CLIPBOARD_TRANSFER,
                AtomEnum::ANY,
                0,
                (MAX_TEXT_BYTES as u32).div_ceil(4),
            )?
            .reply()?)
    }

    fn read_property(&self, deadline: Instant) -> ClipboardResult<Option<String>> {
        let first = self.property()?;
        if first.type_ != self.atoms.INCR {
            return if first.format == 8 && first.bytes_after == 0 {
                decode_text(first.type_, first.value).map(Some)
            } else {
                Ok(None)
            };
        }
        if first.format != 32
            || first.bytes_after != 0
            || first
                .value32()
                .and_then(|mut values| values.next())
                .is_none_or(|length| length as usize > MAX_TEXT_BYTES)
        {
            return Ok(None);
        }

        let mut bytes = Vec::new();
        let mut encoding = None;
        loop {
            self.event_until(deadline, |event| {
                matches!(event, Event::PropertyNotify(event)
                    if event.window == self.window && event.atom == self.atoms.PLAIN_CLIPBOARD_TRANSFER
                        && event.state == Property::NEW_VALUE)
            })?;
            let chunk = self.property()?;
            if chunk.format != 8
                || chunk.bytes_after != 0
                || encoding.is_some_and(|encoding| encoding != chunk.type_)
                || bytes.len() + chunk.value.len() > MAX_TEXT_BYTES
            {
                return Ok(None);
            }
            if chunk.value.is_empty() {
                return decode_text(chunk.type_, bytes).map(Some);
            }
            encoding = Some(chunk.type_);
            bytes.extend(chunk.value);
        }
    }
}

fn decode_text(encoding: Atom, bytes: Vec<u8>) -> ClipboardResult<String> {
    if encoding == AtomEnum::STRING.into() {
        Ok(bytes.into_iter().map(char::from).collect())
    } else {
        Ok(String::from_utf8(bytes)?)
    }
}

enum Command {
    Write(Content, mpsc::SyncSender<ClipboardResult<()>>),
    Stop,
}

pub(super) struct Clipboard {
    conn: Arc<RustConnection>,
    window: Window,
    wake: Atom,
    commands: mpsc::Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl Clipboard {
    pub(super) fn new() -> ClipboardResult<Self> {
        let selection = Selection::connect()?;
        let conn = selection.conn.clone();
        let window = selection.window;
        let wake = selection.atoms.PLAIN_CLIPBOARD_WAKE;
        let (commands, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("x11-clipboard".into())
            .spawn(move || {
                if let Err(error) = Owner::new(selection).run(receiver) {
                    log::warn!("X11 clipboard service stopped: {error}");
                }
            })?;
        Ok(Self {
            conn,
            window,
            wake,
            commands,
            worker: Some(worker),
        })
    }

    pub(super) fn read_text(&self) -> ClipboardResult<Option<String>> {
        Selection::connect()?.read_text()
    }

    pub(super) fn write(&self, content: Content) -> ClipboardResult<()> {
        let (reply, result) = mpsc::sync_channel(1);
        self.send(Command::Write(content, reply))?;
        result.recv()?
    }

    fn send(&self, command: Command) -> ClipboardResult<()> {
        self.commands
            .send(command)
            .map_err(|_| "X11 clipboard service is unavailable")?;
        let event = ClientMessageEvent::new(32, self.window, self.wake, [0u32; 5]);
        self.conn
            .send_event(false, self.window, EventMask::NO_EVENT, event)?
            .ignore_error();
        self.conn.flush()?;
        Ok(())
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        if let Err(error) = self.send(Command::Stop) {
            log::debug!("X11 clipboard shutdown request failed: {error}");
            return;
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                log::warn!("X11 clipboard service panicked");
            }
        }
    }
}

#[derive(Clone)]
struct Payload {
    acquired: u32,
    targets: Vec<Atom>,
    bytes: Arc<[u8]>,
}

struct Transfer {
    payload: Payload,
    target: Atom,
    offset: usize,
    deadline: Instant,
}

impl Transfer {
    fn next(&mut self, limit: usize) -> &[u8] {
        let start = self.offset;
        self.offset = (start + limit).min(self.payload.bytes.len());
        self.deadline = Instant::now() + TIMEOUT;
        &self.payload.bytes[start..self.offset]
    }
}

struct Owner {
    selection: Selection,
    current: Option<Payload>,
    transfers: HashMap<(Window, Atom), Transfer>,
    chunk_size: usize,
}

impl Owner {
    fn new(selection: Selection) -> Self {
        let chunk_size = selection
            .conn
            .maximum_request_bytes()
            .saturating_sub(32)
            .clamp(1, 256 * 1024);
        Self {
            selection,
            current: None,
            transfers: HashMap::new(),
            chunk_size,
        }
    }

    fn run(mut self, receiver: mpsc::Receiver<Command>) -> ClipboardResult<()> {
        loop {
            let event = self.selection.conn.wait_for_event()?;
            self.expire_transfers();
            match event {
                Event::ClientMessage(event)
                    if event.window == self.selection.window
                        && event.type_ == self.selection.atoms.PLAIN_CLIPBOARD_WAKE =>
                {
                    while let Ok(command) = receiver.try_recv() {
                        match command {
                            Command::Write(content, reply) => {
                                let _ = reply.send(self.write(content));
                            }
                            Command::Stop => return self.handoff(),
                        }
                    }
                }
                Event::DestroyNotify(event) if event.window == self.selection.window => {
                    return Ok(());
                }
                event => self.handle(event),
            }
        }
    }

    fn write(&mut self, content: Content) -> ClipboardResult<()> {
        let acquired = self.timestamp()?;
        let Selection {
            conn,
            window,
            atoms,
        } = &self.selection;
        let payload = match content {
            Content::Text(text) => Payload {
                acquired,
                targets: atoms.utf8_targets().into(),
                bytes: Arc::from(text.into_bytes()),
            },
            Content::Png(png) => Payload {
                acquired,
                targets: vec![atoms.PNG],
                bytes: Arc::from(png),
            },
        };
        conn.set_selection_owner(*window, atoms.CLIPBOARD, acquired)?
            .check()?;
        if conn.get_selection_owner(atoms.CLIPBOARD)?.reply()?.owner != *window {
            return Err("X11 refused clipboard ownership".into());
        }
        self.current = Some(payload);
        Ok(())
    }

    fn timestamp(&mut self) -> ClipboardResult<u32> {
        let window = self.selection.window;
        let marker = self.selection.atoms.PLAIN_CLIPBOARD_WAKE;
        self.selection
            .conn
            .change_property8(PropMode::REPLACE, window, marker, AtomEnum::STRING, &[])?
            .check()?;
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            match self.selection.conn.poll_for_event()? {
                Some(Event::PropertyNotify(event))
                    if event.window == window
                        && event.atom == marker
                        && event.state == Property::NEW_VALUE =>
                {
                    return Ok(event.time);
                }
                Some(event) => self.handle(event),
                None => thread::sleep(POLL_INTERVAL),
            }
        }
        Err("X11 clipboard timestamp timed out".into())
    }

    fn handle(&mut self, event: Event) {
        let result = match event {
            Event::SelectionClear(event) if event.selection == self.selection.atoms.CLIPBOARD => {
                self.clear_if_lost()
            }
            Event::SelectionRequest(event) => self.request(event),
            Event::PropertyNotify(event) if event.state == Property::DELETE => {
                self.send_chunk(event.window, event.atom)
            }
            Event::DestroyNotify(event) => {
                self.transfers
                    .retain(|&(window, _), _| window != event.window);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            log::debug!("X11 clipboard request failed: {error}");
        }
    }

    fn clear_if_lost(&mut self) -> ClipboardResult<()> {
        let Selection {
            conn,
            window,
            atoms,
        } = &self.selection;
        if conn.get_selection_owner(atoms.CLIPBOARD)?.reply()?.owner != *window {
            self.current = None;
        }
        Ok(())
    }

    fn request(&mut self, event: SelectionRequestEvent) -> ClipboardResult<()> {
        let property = if event.property == NONE {
            event.target
        } else {
            event.property
        };
        let result = self.provide(event, property);
        let reply = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: event.time,
            requestor: event.requestor,
            selection: event.selection,
            target: event.target,
            property: if matches!(result, Ok(true)) {
                property
            } else {
                NONE
            },
        };
        self.selection
            .conn
            .send_event(false, event.requestor, EventMask::NO_EVENT, reply)?
            .check()?;
        result.map(|_| ())
    }

    fn provide(&mut self, event: SelectionRequestEvent, property: Atom) -> ClipboardResult<bool> {
        let Selection {
            conn,
            window,
            atoms,
        } = &self.selection;
        let Some(payload) = &self.current else {
            return Ok(false);
        };
        if event.owner != *window || event.selection != atoms.CLIPBOARD {
            return Ok(false);
        }
        let key = (event.requestor, property);
        if self.transfers.contains_key(&key) {
            return Ok(false);
        }
        if event.target == atoms.MULTIPLE {
            if event.property == NONE {
                return Ok(false);
            }
            return self.provide_multiple(event, property);
        }
        if event.target == atoms.TIMESTAMP {
            conn.change_property32(
                PropMode::REPLACE,
                event.requestor,
                property,
                AtomEnum::INTEGER,
                &[payload.acquired],
            )?
            .check()?;
        } else if event.target == atoms.TARGETS {
            let mut targets = payload.targets.clone();
            targets.extend([
                atoms.TARGETS,
                atoms.TIMESTAMP,
                atoms.MULTIPLE,
                atoms.SAVE_TARGETS,
            ]);
            conn.change_property32(
                PropMode::REPLACE,
                event.requestor,
                property,
                AtomEnum::ATOM,
                &targets,
            )?
            .check()?;
        } else if !payload.targets.contains(&event.target) {
            return Ok(false);
        } else if payload.bytes.len() <= self.chunk_size {
            conn.change_property8(
                PropMode::REPLACE,
                event.requestor,
                property,
                event.target,
                &payload.bytes,
            )?
            .check()?;
        } else {
            if self.transfers.len() >= MAX_TRANSFERS {
                return Ok(false);
            }
            conn.change_window_attributes(
                event.requestor,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
            )?
            .check()?;
            let size = u32::try_from(payload.bytes.len())?;
            let result = conn
                .change_property32(
                    PropMode::REPLACE,
                    event.requestor,
                    property,
                    atoms.INCR,
                    &[size],
                )?
                .check();
            if let Err(error) = result {
                self.unwatch(event.requestor);
                return Err(error.into());
            }
            self.transfers.insert(
                key,
                Transfer {
                    payload: payload.clone(),
                    target: event.target,
                    offset: 0,
                    deadline: Instant::now() + TIMEOUT,
                },
            );
        }
        Ok(true)
    }

    fn provide_multiple(
        &mut self,
        event: SelectionRequestEvent,
        property: Atom,
    ) -> ClipboardResult<bool> {
        let atom_pair = self.selection.atoms.ATOM_PAIR;
        let multiple = self.selection.atoms.MULTIPLE;
        let reply = self
            .selection
            .conn
            .get_property(false, event.requestor, property, atom_pair, 0, 128)?
            .reply()?;
        if reply.type_ != atom_pair || reply.bytes_after != 0 {
            return Ok(false);
        }
        let Some(values) = reply.value32() else {
            return Ok(false);
        };
        let mut pairs: Vec<_> = values.collect();
        if pairs.len() % 2 != 0 {
            return Ok(false);
        }
        for pair in pairs.chunks_exact_mut(2) {
            let [target, destination] = [pair[0], pair[1]];
            if target == multiple
                || destination == NONE
                || destination == property
                || !matches!(
                    self.provide(
                        SelectionRequestEvent {
                            target,
                            property: destination,
                            ..event
                        },
                        destination
                    ),
                    Ok(true)
                )
            {
                pair[0] = NONE;
                pair[1] = NONE;
            }
        }
        self.selection
            .conn
            .change_property32(
                PropMode::REPLACE,
                event.requestor,
                property,
                atom_pair,
                &pairs,
            )?
            .check()?;
        Ok(true)
    }

    fn send_chunk(&mut self, requestor: Window, property: Atom) -> ClipboardResult<()> {
        let key = (requestor, property);
        let Some(transfer) = self.transfers.get_mut(&key) else {
            return Ok(());
        };
        let target = transfer.target;
        let data = transfer.next(self.chunk_size);
        let result = self
            .selection
            .conn
            .change_property8(PropMode::REPLACE, requestor, property, target, data)?
            .check();
        if data.is_empty() || result.is_err() {
            self.transfers.remove(&key);
            self.unwatch(requestor);
        }
        Ok(result?)
    }

    fn unwatch(&self, requestor: Window) {
        if !self
            .transfers
            .keys()
            .any(|&(window, _)| window == requestor)
        {
            if let Ok(cookie) = self.selection.conn.change_window_attributes(
                requestor,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
            ) {
                cookie.ignore_error();
            }
        }
    }

    fn expire_transfers(&mut self) {
        let now = Instant::now();
        let mut expired = Vec::new();
        self.transfers.retain(|&(window, _), transfer| {
            if transfer.deadline > now {
                return true;
            }
            expired.push(window);
            false
        });
        for window in expired {
            self.unwatch(window);
        }
    }

    fn handoff(&mut self) -> ClipboardResult<()> {
        let Selection {
            conn,
            window,
            atoms,
        } = &self.selection;
        if self.current.is_none()
            || conn.get_selection_owner(atoms.CLIPBOARD)?.reply()?.owner != *window
            || conn
                .get_selection_owner(atoms.CLIPBOARD_MANAGER)?
                .reply()?
                .owner
                == NONE
        {
            return Ok(());
        }
        self.selection
            .convert(atoms.CLIPBOARD_MANAGER, atoms.SAVE_TARGETS)?;
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            match self.selection.conn.poll_for_event()? {
                Some(Event::SelectionNotify(event))
                    if event.requestor == self.selection.window
                        && event.selection == self.selection.atoms.CLIPBOARD_MANAGER
                        && event.target == self.selection.atoms.SAVE_TARGETS =>
                {
                    return Ok(());
                }
                Some(event) => self.handle(event),
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        Err("X11 clipboard manager handoff timed out".into())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/host/clipboard_x11.rs"]
mod tests;
