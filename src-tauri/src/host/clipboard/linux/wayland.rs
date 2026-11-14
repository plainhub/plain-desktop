use std::{
    collections::{BTreeMap, HashMap},
    io::{self, Read},
    net::Shutdown,
    os::{
        fd::{AsFd, OwnedFd},
        unix::net::UnixStream,
    },
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    io::Errno,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::{ObjectId, WaylandError},
    protocol::{wl_callback, wl_registry, wl_seat},
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1 as ext_device, ext_data_control_manager_v1 as ext_manager,
    ext_data_control_offer_v1 as ext_offer, ext_data_control_source_v1 as ext_source,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as wlr_device, zwlr_data_control_manager_v1 as wlr_manager,
    zwlr_data_control_offer_v1 as wlr_offer, zwlr_data_control_source_v1 as wlr_source,
};

use super::{ClipboardResult, Content, MAX_TEXT_BYTES};

const TIMEOUT: Duration = Duration::from_secs(4);
const PASTE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PASTES: usize = 8;
const UTF8_MIME: &str = "text/plain;charset=utf-8";
const TEXT_TYPES: &[&str] = &[UTF8_MIME, "UTF8_STRING", "text/plain", "TEXT", "STRING"];
const IMAGE_TYPES: &[&str] = &["image/png"];

pub(super) struct Clipboard {
    publisher: Option<Publisher>,
}

impl Clipboard {
    pub(super) fn new() -> ClipboardResult<Self> {
        Session::connect()?;
        Ok(Self { publisher: None })
    }

    pub(super) fn read_text(&self) -> ClipboardResult<Option<String>> {
        Session::connect()?.read_text()
    }

    pub(super) fn write(&mut self, content: Content) -> ClipboardResult<()> {
        self.publish(Session::connect()?, content)
    }

    fn publish(&mut self, mut session: Session, content: Content) -> ClipboardResult<()> {
        let (stop, wake) = UnixStream::pair()?;
        let (ready, result) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("wayland-clipboard".into())
            .spawn(move || match session.set_content(content) {
                Ok(()) => {
                    if ready.send(Ok(())).is_ok() {
                        if let Err(error) = session.serve(&wake) {
                            log::debug!("Wayland clipboard source stopped: {error}");
                        }
                    }
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })?;
        let publisher = Publisher {
            stop,
            worker: Some(worker),
        };
        result.recv()??;
        self.publisher = Some(publisher);
        Ok(())
    }
}

struct Publisher {
    stop: UnixStream,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Publisher {
    fn drop(&mut self) {
        let _ = self.stop.shutdown(Shutdown::Write);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                log::warn!("Wayland clipboard source panicked");
            }
        }
    }
}

struct Session {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    manager: Manager,
}

impl Session {
    fn connect() -> ClipboardResult<Self> {
        Self::from_connection(Connection::connect_to_env()?)
    }

    fn from_connection(connection: Connection) -> ClipboardResult<Self> {
        let mut queue = connection.new_event_queue();
        let mut state = State::default();
        let handle = queue.handle();
        let registry = connection.display().get_registry(&handle, ());
        roundtrip(&connection, &mut queue, &mut state)?;
        let advertised = |interface| {
            state.globals.iter().find_map(|(&name, (global, version))| {
                (global == interface && *version > 0).then_some((name, *version))
            })
        };
        let manager = if let Some((name, _)) =
            advertised(ext_manager::ExtDataControlManagerV1::interface().name)
        {
            Manager::Ext(registry.bind(name, 1, &handle, ()))
        } else if let Some((name, version)) =
            advertised(wlr_manager::ZwlrDataControlManagerV1::interface().name)
        {
            Manager::Wlr(registry.bind(name, version.min(2), &handle, ()))
        } else {
            return Err("Wayland compositor does not advertise clipboard data control".into());
        };

        for (&name, (interface, version)) in &state.globals {
            if interface == wl_seat::WlSeat::interface().name && *version > 0 {
                let seat = registry.bind(name, 1, &handle, ());
                let device = match &manager {
                    Manager::Ext(manager) => {
                        Device::Ext(manager.get_data_device(&seat, &handle, name))
                    }
                    Manager::Wlr(manager) => {
                        Device::Wlr(manager.get_data_device(&seat, &handle, name))
                    }
                };
                state.seats.insert(
                    name,
                    Seat {
                        device,
                        selection: None,
                    },
                );
            }
        }
        roundtrip(&connection, &mut queue, &mut state)?;
        if state.seats.is_empty() {
            return Err("Wayland clipboard data control has no active seats".into());
        }
        Ok(Self {
            connection,
            queue,
            state,
            manager,
        })
    }

    fn read_text(mut self) -> ClipboardResult<Option<String>> {
        let Some(offer) = self
            .state
            .seats
            .values()
            .next()
            .and_then(|seat| seat.selection.as_ref())
            .and_then(|id| self.state.offers.get(id))
        else {
            return Ok(None);
        };
        let Some((_, mime)) = &offer.text_mime else {
            return Ok(None);
        };
        let (reader, writer) = UnixStream::pair()?;
        match &offer.proxy {
            OfferProxy::Ext(offer) => offer.receive(mime.clone(), writer.as_fd()),
            OfferProxy::Wlr(offer) => offer.receive(mime.clone(), writer.as_fd()),
        }
        drop(writer);
        roundtrip(&self.connection, &mut self.queue, &mut self.state)?;
        read_text(reader)
    }

    fn set_content(&mut self, content: Content) -> ClipboardResult<()> {
        let (bytes, types) = match content {
            Content::Text(text) => (text.into_bytes(), TEXT_TYPES),
            Content::Png(png) => (png, IMAGE_TYPES),
        };
        let payload = Arc::new(Payload {
            bytes,
            types,
            transfers: AtomicUsize::new(0),
        });
        let handle = self.queue.handle();
        for seat in self.state.seats.values() {
            match (&self.manager, &seat.device) {
                (Manager::Ext(manager), Device::Ext(device)) => {
                    let source = manager.create_data_source(&handle, payload.clone());
                    for mime in types {
                        source.offer((*mime).into());
                    }
                    device.set_selection(Some(&source));
                }
                (Manager::Wlr(manager), Device::Wlr(device)) => {
                    let source = manager.create_data_source(&handle, payload.clone());
                    for mime in types {
                        source.offer((*mime).into());
                    }
                    device.set_selection(Some(&source));
                }
                _ => unreachable!("all seats use the selected clipboard protocol"),
            }
            self.state.sources += 1;
        }
        roundtrip(&self.connection, &mut self.queue, &mut self.state)?;
        if self.state.sources == 0 || self.state.seats.is_empty() {
            return Err("Wayland compositor canceled the clipboard source".into());
        }
        Ok(())
    }

    fn serve(&mut self, stop: &UnixStream) -> ClipboardResult<()> {
        loop {
            self.queue.dispatch_pending(&mut self.state)?;
            if self.state.sources == 0 || self.state.seats.is_empty() {
                self.queue.flush()?;
                return Ok(());
            }
            if !wait_for_events(&self.queue, Some(stop), None)? {
                return Ok(());
            }
        }
    }
}

fn roundtrip(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
) -> ClipboardResult<()> {
    let done = Arc::new(AtomicBool::new(false));
    connection.display().sync(&queue.handle(), done.clone());
    let deadline = Instant::now() + TIMEOUT;
    loop {
        queue.dispatch_pending(state)?;
        if done.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !wait_for_events(queue, None, Some(deadline))? {
            return Err("Wayland clipboard roundtrip timed out".into());
        }
    }
}

fn wait_for_events(
    queue: &EventQueue<State>,
    stop: Option<&UnixStream>,
    deadline: Option<Instant>,
) -> ClipboardResult<bool> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Ok(false);
    }
    let Some(read) = queue.prepare_read() else {
        return Ok(true);
    };
    let mut flags = PollFlags::IN;
    match queue.flush() {
        Ok(()) => {}
        Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
            flags |= PollFlags::OUT
        }
        Err(error) => return Err(error.into()),
    }
    let socket = read.connection_fd();
    let wake = stop.map(AsFd::as_fd).unwrap_or(socket);
    let mut fds = [
        PollFd::new(&socket, flags),
        PollFd::new(&wake, PollFlags::IN),
    ];
    let count = if stop.is_some() { 2 } else { 1 };
    if !poll_until(&mut fds[..count], deadline)? || !fds[1].revents().is_empty() {
        return Ok(false);
    }
    if fds[0]
        .revents()
        .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL)
    {
        read.read()?;
    }
    Ok(true)
}

fn poll_until(fds: &mut [PollFd<'_>], deadline: Option<Instant>) -> io::Result<bool> {
    loop {
        let timeout = if let Some(deadline) = deadline {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(false);
            };
            Some(Timespec {
                tv_sec: remaining.as_secs() as _,
                tv_nsec: remaining.subsec_nanos() as _,
            })
        } else {
            None
        };
        match poll(fds, timeout.as_ref()) {
            Ok(count) => return Ok(count != 0),
            Err(Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn read_text(mut reader: UnixStream) -> ClipboardResult<Option<String>> {
    reader.set_nonblocking(true)?;
    let deadline = Instant::now() + TIMEOUT;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    while poll_until(&mut [PollFd::new(&reader, PollFlags::IN)], Some(deadline))? {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(Some(String::from_utf8(bytes)?)),
            Ok(count) => {
                if bytes.len() + count > MAX_TEXT_BYTES {
                    return Ok(None);
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err("Wayland clipboard read timed out".into())
}

struct Payload {
    bytes: Vec<u8>,
    types: &'static [&'static str],
    transfers: AtomicUsize,
}

impl Payload {
    fn send(self: &Arc<Self>, mime: &str, fd: OwnedFd) {
        if !self.types.contains(&mime)
            || self
                .transfers
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                    (count < MAX_PASTES).then_some(count + 1)
                })
                .is_err()
        {
            return;
        }
        let transfer = Transfer(self.clone());
        if let Err(error) = thread::Builder::new()
            .name("wayland-clipboard-paste".into())
            .spawn(move || {
                if let Err(error) = write_to(fd, &transfer.0.bytes) {
                    log::debug!("Wayland clipboard paste failed: {error}");
                }
            })
        {
            log::debug!("Wayland clipboard paste worker unavailable: {error}");
        }
    }
}

struct Transfer(Arc<Payload>);

impl Drop for Transfer {
    fn drop(&mut self) {
        self.0.transfers.fetch_sub(1, Ordering::Relaxed);
    }
}

fn write_to(fd: OwnedFd, mut bytes: &[u8]) -> io::Result<()> {
    fcntl_setfl(&fd, fcntl_getfl(&fd)? | OFlags::NONBLOCK)?;
    let deadline = Instant::now() + PASTE_TIMEOUT;
    while !bytes.is_empty() {
        if !poll_until(&mut [PollFd::new(&fd, PollFlags::OUT)], Some(deadline))? {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Wayland clipboard paste timed out",
            ));
        }
        match rustix::io::write(&fd, bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(Errno::INTR | Errno::AGAIN) => {}
            Err(Errno::PIPE) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

enum Manager {
    Ext(ext_manager::ExtDataControlManagerV1),
    Wlr(wlr_manager::ZwlrDataControlManagerV1),
}

enum Device {
    Ext(ext_device::ExtDataControlDeviceV1),
    Wlr(wlr_device::ZwlrDataControlDeviceV1),
}

enum OfferProxy {
    Ext(ext_offer::ExtDataControlOfferV1),
    Wlr(wlr_offer::ZwlrDataControlOfferV1),
}

struct Offer {
    proxy: OfferProxy,
    text_mime: Option<(u8, String)>,
}

struct Seat {
    device: Device,
    selection: Option<ObjectId>,
}

#[derive(Default)]
struct State {
    globals: BTreeMap<u32, (String, u32)>,
    seats: BTreeMap<u32, Seat>,
    offers: HashMap<ObjectId, Offer>,
    sources: usize,
}

impl State {
    fn discard_offer(&mut self, id: &ObjectId) {
        if let Some(offer) = self.offers.remove(id) {
            match offer.proxy {
                OfferProxy::Ext(offer) => offer.destroy(),
                OfferProxy::Wlr(offer) => offer.destroy(),
            }
        }
    }

    fn set_selection(&mut self, seat: u32, selection: Option<ObjectId>) {
        if let Some(seat) = self.seats.get_mut(&seat) {
            if seat.selection != selection {
                if let Some(previous) = std::mem::replace(&mut seat.selection, selection) {
                    self.discard_offer(&previous);
                }
            }
        } else if let Some(id) = selection {
            self.discard_offer(&id);
        }
    }

    fn remove_seat(&mut self, name: u32) {
        if let Some(seat) = self.seats.remove(&name) {
            if let Some(selection) = seat.selection {
                self.discard_offer(&selection);
            }
            match seat.device {
                Device::Ext(device) => device.destroy(),
                Device::Wlr(device) => device.destroy(),
            }
        }
    }

    fn record_mime(&mut self, offer: ObjectId, mime: String) {
        let Some(offer) = self.offers.get_mut(&offer) else {
            return;
        };
        let Some(priority) = text_priority(&mime) else {
            return;
        };
        if offer
            .text_mime
            .as_ref()
            .is_none_or(|(previous, _)| priority < *previous)
        {
            offer.text_mime = Some((priority, mime));
        }
    }
}

fn text_priority(mime: &str) -> Option<u8> {
    if mime == UTF8_MIME {
        Some(0)
    } else if mime == "UTF8_STRING" {
        Some(1)
    } else if matches!(mime, "TEXT" | "STRING")
        || mime.starts_with("text/")
        || mime.contains("json")
        || ["script", "xml", "yaml", "csv", "ini"]
            .iter()
            .any(|suffix| mime.ends_with(suffix))
    {
        Some(2)
    } else {
        None
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                state.globals.insert(name, (interface, version));
            }
            wl_registry::Event::GlobalRemove { name } => {
                state.globals.remove(&name);
                state.remove_seat(name);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> for State {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Relaxed);
    }
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore ext_manager::ExtDataControlManagerV1);
wayland_client::delegate_noop!(State: ignore wlr_manager::ZwlrDataControlManagerV1);

macro_rules! data_control_events {
    ($device:ty, $device_event:ident, $offer:ty, $offer_event:ident, $source:ty, $source_event:ident, $variant:ident) => {
        impl Dispatch<$device, u32> for State {
            fn event(state: &mut Self, _: &$device, event: $device_event::Event, seat: &u32, _: &Connection, _: &QueueHandle<Self>) {
                match event {
                    $device_event::Event::DataOffer { id } => {
                        state.offers.insert(id.id(), Offer { proxy: OfferProxy::$variant(id), text_mime: None });
                    }
                    $device_event::Event::Selection { id } => state.set_selection(*seat, id.map(|offer| offer.id())),
                    $device_event::Event::PrimarySelection { id: Some(offer) } => state.discard_offer(&offer.id()),
                    $device_event::Event::Finished => state.remove_seat(*seat),
                    _ => {}
                }
            }

            wayland_client::event_created_child!(State, $device, [$device_event::EVT_DATA_OFFER_OPCODE => ($offer, ())]);
        }

        impl Dispatch<$offer, ()> for State {
            fn event(state: &mut Self, offer: &$offer, event: $offer_event::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                if let $offer_event::Event::Offer { mime_type } = event {
                    state.record_mime(offer.id(), mime_type);
                }
            }
        }

        impl Dispatch<$source, Arc<Payload>> for State {
            fn event(state: &mut Self, source: &$source, event: $source_event::Event, payload: &Arc<Payload>, _: &Connection, _: &QueueHandle<Self>) {
                match event {
                    $source_event::Event::Send { mime_type, fd } => payload.send(&mime_type, fd),
                    $source_event::Event::Cancelled => {
                        source.destroy();
                        state.sources -= 1;
                    }
                    _ => {}
                }
            }
        }
    };
}

data_control_events!(
    ext_device::ExtDataControlDeviceV1,
    ext_device,
    ext_offer::ExtDataControlOfferV1,
    ext_offer,
    ext_source::ExtDataControlSourceV1,
    ext_source,
    Ext
);
data_control_events!(
    wlr_device::ZwlrDataControlDeviceV1,
    wlr_device,
    wlr_offer::ZwlrDataControlOfferV1,
    wlr_offer,
    wlr_source::ZwlrDataControlSourceV1,
    wlr_source,
    Wlr
);

#[cfg(test)]
#[path = "../../../../tests/unit/host/clipboard_wayland.rs"]
mod tests;
