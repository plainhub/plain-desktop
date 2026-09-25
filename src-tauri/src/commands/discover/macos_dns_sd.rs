use super::MdnsActivity;
use plain_rs::mdns::service_browser::{FoundDevice, MdnsServiceSnapshot};
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString, c_char, c_void};
use std::net::Ipv4Addr;
use std::ptr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type DnsRef = *mut c_void;
const ADD: u32 = 2;
const MORE_COMING: u32 = 1;
const IPV4: u32 = 1;
const SERVICE_TYPE: &str = "_plainapp._tcp";

type BrowseReply = unsafe extern "C" fn(
    DnsRef,
    u32,
    u32,
    i32,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_void,
);
type ResolveReply = unsafe extern "C" fn(
    DnsRef,
    u32,
    u32,
    i32,
    *const c_char,
    *const c_char,
    u16,
    u16,
    *const u8,
    *mut c_void,
);
type AddressReply = unsafe extern "C" fn(
    DnsRef,
    u32,
    u32,
    i32,
    *const c_char,
    *const libc::sockaddr,
    u32,
    *mut c_void,
);

unsafe extern "C" {
    fn DNSServiceBrowse(
        reference: *mut DnsRef,
        flags: u32,
        interface: u32,
        service_type: *const c_char,
        domain: *const c_char,
        callback: BrowseReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceResolve(
        reference: *mut DnsRef,
        flags: u32,
        interface: u32,
        name: *const c_char,
        service_type: *const c_char,
        domain: *const c_char,
        callback: ResolveReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceGetAddrInfo(
        reference: *mut DnsRef,
        flags: u32,
        interface: u32,
        protocol: u32,
        hostname: *const c_char,
        callback: AddressReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceRefSockFD(reference: DnsRef) -> i32;
    fn DNSServiceProcessResult(reference: DnsRef) -> i32;
    fn DNSServiceRefDeallocate(reference: DnsRef);
}

struct ServiceRef(DnsRef);

impl Drop for ServiceRef {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { DNSServiceRefDeallocate(self.0) };
        }
    }
}

#[derive(Clone)]
struct ServiceName {
    name: String,
    service_type: String,
    domain: String,
    interface: u32,
}

impl ServiceName {
    fn key(&self) -> String {
        format!("{}|{}|{}", self.name, self.domain, self.interface)
    }
}

struct BrowseEvent {
    service: ServiceName,
    added: bool,
}

struct ResolvedService {
    fullname: String,
    hostname: String,
    port: u16,
    txt: Vec<String>,
}

struct State {
    snapshots: Mutex<HashMap<String, MdnsServiceSnapshot>>,
    activity: Mutex<VecDeque<MdnsActivity>>,
    started: AtomicBool,
}

#[derive(Clone)]
pub struct MacDnsSdBrowser {
    state: Arc<State>,
    on_device: Arc<dyn Fn(FoundDevice) + Send + Sync>,
}

impl MacDnsSdBrowser {
    pub fn new(on_device: impl Fn(FoundDevice) + Send + Sync + 'static) -> Self {
        Self {
            state: Arc::new(State {
                snapshots: Mutex::new(HashMap::new()),
                activity: Mutex::new(VecDeque::new()),
                started: AtomicBool::new(false),
            }),
            on_device: Arc::new(on_device),
        }
    }

    pub fn start(&self) {
        if self.state.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let browser = self.clone();
        std::thread::Builder::new()
            .name("macos-dns-sd-browser".into())
            .spawn(move || browser.browse_loop())
            .expect("spawn macOS DNS-SD browser");
    }

    pub fn snapshot(&self) -> Vec<MdnsServiceSnapshot> {
        let mut values: Vec<_> = merged_snapshots(self.state.snapshots.lock().unwrap().values())
            .into_values()
            .collect();
        values.sort_by(|a, b| a.instance_fqdn.cmp(&b.instance_fqdn));
        values
    }

    pub fn activity(&self) -> Vec<MdnsActivity> {
        self.state
            .activity
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    fn record(&self, event: &str, detail: impl Into<String>) {
        let mut activity = self.state.activity.lock().unwrap();
        activity.push_front(MdnsActivity {
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            event: event.to_string(),
            detail: detail.into(),
        });
        activity.truncate(100);
    }

    fn browse_loop(&self) {
        loop {
            let (tx, rx) = mpsc::channel::<BrowseEvent>();
            let context = Box::into_raw(Box::new(tx));
            let mut raw = ptr::null_mut();
            let service_type = CString::new(SERVICE_TYPE).unwrap();
            let result = unsafe {
                DNSServiceBrowse(
                    &mut raw,
                    0,
                    0,
                    service_type.as_ptr(),
                    ptr::null(),
                    browse_callback,
                    context.cast(),
                )
            };
            if result != 0 {
                self.record("search_failed", format!("DNSServiceBrowse error {result}"));
                unsafe { drop(Box::from_raw(context)) };
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            let reference = ServiceRef(raw);
            self.record("search_started", "_plainapp._tcp.local");
            let mut active: HashMap<String, Arc<AtomicBool>> = HashMap::new();
            loop {
                if process_one(&reference, 500).is_err() {
                    break;
                }
                while let Ok(event) = rx.try_recv() {
                    let key = event.service.key();
                    if event.added {
                        if active.contains_key(&key) {
                            continue;
                        }
                        self.record(
                            "service_found",
                            format!(
                                "{} on interface {}",
                                event.service.name, event.service.interface
                            ),
                        );
                        let flag = Arc::new(AtomicBool::new(true));
                        active.insert(key.clone(), flag.clone());
                        let browser = self.clone();
                        std::thread::spawn(move || browser.resolve_loop(event.service, flag));
                    } else {
                        self.record("service_removed", &event.service.name);
                        if let Some(flag) = active.remove(&key) {
                            flag.store(false, Ordering::SeqCst);
                        }
                        self.state.snapshots.lock().unwrap().remove(&key);
                    }
                }
            }
            for flag in active.values() {
                flag.store(false, Ordering::SeqCst);
            }
            self.state.snapshots.lock().unwrap().clear();
            drop(reference);
            unsafe { drop(Box::from_raw(context)) };
            self.record("search_restarted", "DNS-SD connection closed");
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn resolve_loop(&self, service: ServiceName, active: Arc<AtomicBool>) {
        while active.load(Ordering::SeqCst) {
            match resolve(&service) {
                Ok(resolved) => {
                    let ips = addresses(&resolved.hostname, service.interface);
                    if !active.load(Ordering::SeqCst) {
                        break;
                    }
                    let mut snapshot = MdnsServiceSnapshot {
                        service_type: format!(
                            "{}.{}",
                            service.service_type.trim_end_matches('.'),
                            service.domain.trim_end_matches('.')
                        ),
                        instance_name: service.name.clone(),
                        instance_fqdn: resolved.fullname.clone(),
                        hostname: resolved.hostname.clone(),
                        port: resolved.port,
                        txt_records: resolved.txt.clone(),
                        ips: ips.clone(),
                        ipv6: vec![],
                        complete: resolved.port > 0
                            && !ips.is_empty()
                            && resolved.txt.iter().any(|v| v.starts_with("id=")),
                    };
                    let (changed, all_ips) = {
                        let mut snapshots = self.state.snapshots.lock().unwrap();
                        if snapshot.ips.is_empty()
                            && let Some(previous) = snapshots.get(&service.key())
                            && previous.hostname == snapshot.hostname
                        {
                            snapshot.ips = previous.ips.clone();
                            snapshot.complete = previous.complete;
                        }
                        let old = snapshots.insert(service.key(), snapshot.clone());
                        let changed = old
                            .as_ref()
                            .map(|s| {
                                s.hostname != snapshot.hostname
                                    || s.port != snapshot.port
                                    || s.ips != snapshot.ips
                                    || s.txt_records != snapshot.txt_records
                            })
                            .unwrap_or(true);
                        let all_ips = merged_snapshots(snapshots.values())
                            .get(&snapshot.instance_fqdn.to_lowercase())
                            .map(|s| s.ips.clone())
                            .unwrap_or_default();
                        (changed, all_ips)
                    };
                    if changed {
                        self.record(
                            "details_updated",
                            format!(
                                "{} → {}:{} [{}] TXT {}",
                                service.name,
                                resolved.hostname,
                                resolved.port,
                                ips.join(", "),
                                resolved.txt.join(", ")
                            ),
                        );
                    }
                    if let Some(device) = found_device(&service, &resolved, all_ips) {
                        (self.on_device)(device);
                    }
                }
                Err(error) => self.record("details_failed", format!("{}: {error}", service.name)),
            }
            for _ in 0..50 {
                if !active.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn merged_snapshots<'a>(
    snapshots: impl Iterator<Item = &'a MdnsServiceSnapshot>,
) -> HashMap<String, MdnsServiceSnapshot> {
    let mut merged: HashMap<String, MdnsServiceSnapshot> = HashMap::new();
    for snapshot in snapshots {
        let key = snapshot.instance_fqdn.to_lowercase();
        match merged.get_mut(&key) {
            Some(existing) => {
                existing.ips.extend(snapshot.ips.iter().cloned());
                existing.ips.sort();
                existing.ips.dedup();
                existing.ipv6.extend(snapshot.ipv6.iter().cloned());
                existing.ipv6.sort();
                existing.ipv6.dedup();
                existing.complete |= snapshot.complete;
            }
            None => {
                merged.insert(key, snapshot.clone());
            }
        }
    }
    merged
}

fn found_device(
    service: &ServiceName,
    resolved: &ResolvedService,
    ips: Vec<String>,
) -> Option<FoundDevice> {
    let value = |key: &str| {
        resolved
            .txt
            .iter()
            .find_map(|entry| entry.strip_prefix(key).map(str::to_owned))
            .unwrap_or_default()
    };
    let id = value("id=");
    if id.is_empty() || resolved.port == 0 || ips.is_empty() {
        return None;
    }
    Some(FoundDevice {
        id,
        name: service.name.clone(),
        ips,
        ipv6: vec![],
        port: resolved.port,
        device_type: value("dv="),
        version: value("ver="),
        platform: value("pf="),
    })
}

fn process_one(reference: &ServiceRef, timeout_ms: i32) -> Result<(), i32> {
    let fd = unsafe { DNSServiceRefSockFD(reference.0) };
    if fd < 0 {
        return Err(fd);
    }
    let mut poll_fd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut poll_fd, 1, timeout_ms) };
    if ready < 0 {
        return Err(ready);
    }
    if ready > 0 {
        let result = unsafe { DNSServiceProcessResult(reference.0) };
        if result != 0 {
            return Err(result);
        }
    }
    Ok(())
}

unsafe extern "C" fn browse_callback(
    _: DnsRef,
    flags: u32,
    interface: u32,
    error: i32,
    name: *const c_char,
    service_type: *const c_char,
    domain: *const c_char,
    context: *mut c_void,
) {
    if error != 0 || name.is_null() || service_type.is_null() || domain.is_null() {
        return;
    }
    let sender = unsafe { &*(context as *const mpsc::Sender<BrowseEvent>) };
    let event = BrowseEvent {
        service: ServiceName {
            name: unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned(),
            service_type: unsafe { CStr::from_ptr(service_type) }
                .to_string_lossy()
                .into_owned(),
            domain: unsafe { CStr::from_ptr(domain) }
                .to_string_lossy()
                .into_owned(),
            interface,
        },
        added: flags & ADD != 0,
    };
    let _ = sender.send(event);
}

fn resolve(service: &ServiceName) -> Result<ResolvedService, String> {
    let name = CString::new(service.name.as_str()).map_err(|e| e.to_string())?;
    let service_type = CString::new(service.service_type.as_str()).map_err(|e| e.to_string())?;
    let domain = CString::new(service.domain.as_str()).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel::<ResolvedService>();
    let context = Box::into_raw(Box::new(tx));
    let mut raw = ptr::null_mut();
    let result = unsafe {
        DNSServiceResolve(
            &mut raw,
            0,
            service.interface,
            name.as_ptr(),
            service_type.as_ptr(),
            domain.as_ptr(),
            resolve_callback,
            context.cast(),
        )
    };
    if result != 0 {
        unsafe { drop(Box::from_raw(context)) };
        return Err(format!("DNSServiceResolve error {result}"));
    }
    let reference = ServiceRef(raw);
    for _ in 0..20 {
        if let Err(error) = process_one(&reference, 100) {
            unsafe { drop(Box::from_raw(context)) };
            return Err(format!("DNSServiceProcessResult error {error}"));
        }
        if let Ok(value) = rx.try_recv() {
            unsafe { drop(Box::from_raw(context)) };
            return Ok(value);
        }
    }
    unsafe { drop(Box::from_raw(context)) };
    Err("timed out".into())
}

unsafe extern "C" fn resolve_callback(
    _: DnsRef,
    _: u32,
    _: u32,
    error: i32,
    fullname: *const c_char,
    hostname: *const c_char,
    port: u16,
    txt_len: u16,
    txt: *const u8,
    context: *mut c_void,
) {
    if error != 0 || fullname.is_null() || hostname.is_null() {
        return;
    }
    let sender = unsafe { &*(context as *const mpsc::Sender<ResolvedService>) };
    let bytes = if txt.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(txt, txt_len as usize) }
    };
    let _ = sender.send(ResolvedService {
        fullname: unsafe { CStr::from_ptr(fullname) }
            .to_string_lossy()
            .into_owned(),
        hostname: unsafe { CStr::from_ptr(hostname) }
            .to_string_lossy()
            .into_owned(),
        port: u16::from_be(port),
        txt: parse_txt(bytes),
    });
}

fn parse_txt(mut bytes: &[u8]) -> Vec<String> {
    let mut records = Vec::new();
    while let Some((&length, rest)) = bytes.split_first() {
        if rest.len() < length as usize {
            break;
        }
        records.push(String::from_utf8_lossy(&rest[..length as usize]).into_owned());
        bytes = &rest[length as usize..];
    }
    records
}

fn addresses(hostname: &str, interface: u32) -> Vec<String> {
    let Ok(host) = CString::new(hostname) else {
        return vec![];
    };
    let (tx, rx) = mpsc::channel::<(String, bool)>();
    let context = Box::into_raw(Box::new(tx));
    let mut raw = ptr::null_mut();
    let result = unsafe {
        DNSServiceGetAddrInfo(
            &mut raw,
            0,
            interface,
            IPV4,
            host.as_ptr(),
            address_callback,
            context.cast(),
        )
    };
    if result != 0 {
        unsafe { drop(Box::from_raw(context)) };
        return vec![];
    }
    let reference = ServiceRef(raw);
    let mut ips = Vec::new();
    for _ in 0..20 {
        if process_one(&reference, 100).is_err() {
            break;
        }
        let mut more = false;
        while let Ok((ip, has_more)) = rx.try_recv() {
            if !ips.contains(&ip) {
                ips.push(ip);
            }
            more = has_more;
        }
        if !ips.is_empty() && !more {
            break;
        }
    }
    unsafe { drop(Box::from_raw(context)) };
    ips.sort();
    ips
}

unsafe extern "C" fn address_callback(
    _: DnsRef,
    flags: u32,
    _: u32,
    error: i32,
    _: *const c_char,
    address: *const libc::sockaddr,
    _: u32,
    context: *mut c_void,
) {
    if error != 0
        || flags & ADD == 0
        || address.is_null()
        || unsafe { (*address).sa_family } != libc::AF_INET as u8
    {
        return;
    }
    let ip = unsafe { (*(address as *const libc::sockaddr_in)).sin_addr.s_addr };
    let sender = unsafe { &*(context as *const mpsc::Sender<(String, bool)>) };
    let _ = sender.send((
        Ipv4Addr::from(ip.to_ne_bytes()).to_string(),
        flags & MORE_COMING != 0,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dns_sd_txt_and_creates_discovered_device() {
        let txt = parse_txt(&[
            16, b'i', b'd', b'=', b'1', b'v', b'm', b'2', b'0', b'q', b'x', b'4', b's', b'x', b'6',
            b'y', b'6', 8, b'd', b'v', b'=', b'P', b'H', b'O', b'N', b'E',
        ]);
        let service = ServiceName {
            name: "Galaxy S20 5G".into(),
            service_type: SERVICE_TYPE.into(),
            domain: "local.".into(),
            interface: 17,
        };
        let resolved = ResolvedService {
            fullname: "Galaxy\\032S20\\0325G._plainapp._tcp.local.".into(),
            hostname: "ap.local.".into(),
            port: 8443,
            txt,
        };
        let device = found_device(&service, &resolved, vec!["192.168.123.29".into()]).unwrap();
        assert_eq!(device.id, "1vm20qx4sx6y6");
        assert_eq!(device.name, "Galaxy S20 5G");
        assert_eq!(device.ips, ["192.168.123.29"]);
        assert_eq!(device.port, 8443);
        assert_eq!(device.device_type, "PHONE");
    }

    #[test]
    fn combines_addresses_for_one_service_on_multiple_interfaces() {
        let first = MdnsServiceSnapshot {
            service_type: "_plainapp._tcp.local".into(),
            instance_name: "Phone".into(),
            instance_fqdn: "Phone._plainapp._tcp.local.".into(),
            hostname: "phone.local.".into(),
            port: 8443,
            txt_records: vec!["id=phone-1".into()],
            ips: vec!["192.168.1.2".into()],
            ipv6: vec![],
            complete: true,
        };
        let second = MdnsServiceSnapshot {
            ips: vec!["10.0.0.2".into()],
            ..first.clone()
        };
        let combined = merged_snapshots([&first, &second].into_iter());
        assert_eq!(combined.len(), 1);
        assert_eq!(
            combined["phone._plainapp._tcp.local."].ips,
            ["10.0.0.2", "192.168.1.2"]
        );
    }
}
