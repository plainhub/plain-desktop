use plain_libs::serde_json::{Value, json};
use plain_libs::utils::base64::{base64_decode_checked, base64_encode};
use plain_libs::utils::uuid::Uuid;
use plain_rs::features::discovery::advertisement::decode_ble;
use plain_rs::infrastructure::protocols::ble;
use plain_server::host::events::HostEvents;
use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tauri::{AppHandle, WebviewWindow, ipc::Channel};
use tokio::sync::{mpsc, oneshot};

unsafe extern "C" {
    fn plain_macos_ble_start(
        state: extern "C" fn(isize),
        advertisement: extern "C" fn(*const c_char, *const c_char) -> bool,
        frame: extern "C" fn(*const u8, usize),
        completion: extern "C" fn(u64, bool),
    );
    fn plain_macos_ble_stop();
    fn plain_macos_ble_connect(
        operation: u64,
        identifier: *const c_char,
        service: *const c_char,
        characteristic: *const c_char,
    );
    fn plain_macos_ble_write(operation: u64, bytes: *const u8, size: usize);
    fn plain_macos_ble_disconnect();
}

static RADIO: OnceLock<Radio> = OnceLock::new();

struct Scan {
    owner: String,
    states: Channel<&'static str>,
    advertisements: mpsc::Sender<(String, String)>,
    canceled: oneshot::Sender<()>,
}

#[derive(Default)]
struct State {
    scan: Option<Scan>,
    available: bool,
    peers: HashMap<String, String>,
    operations: HashMap<u64, oneshot::Sender<bool>>,
    frames: Option<mpsc::Sender<Vec<u8>>>,
}

struct Radio {
    app: AppHandle,
    host: Arc<HostEvents>,
    state: Mutex<State>,
    link: tokio::sync::Mutex<()>,
    pairing: tokio::sync::Mutex<Option<(String, Link)>>,
    operation: AtomicU64,
}

pub fn initialize(app: AppHandle, host: Arc<HostEvents>) -> Result<(), String> {
    RADIO
        .set(Radio {
            app,
            host,
            state: Mutex::default(),
            link: tokio::sync::Mutex::default(),
            pairing: tokio::sync::Mutex::default(),
            operation: AtomicU64::new(1),
        })
        .map_err(|_| "Bluetooth host already initialized".into())
}

fn radio() -> &'static Radio {
    RADIO.get().expect("Bluetooth host initialized at startup")
}

extern "C" fn state_changed(value: isize) {
    let (channel, status) = {
        let mut state = radio().state.lock().unwrap();
        state.available = value == 5;
        if !state.available {
            state.peers.clear();
            state.frames.take();
        }
        let status = match value {
            5 => "scanning",
            4 => "off",
            3 => "denied",
            2 => "unsupported",
            _ => "starting",
        };
        (state.scan.as_ref().map(|scan| scan.states.clone()), status)
    };
    if let Some(channel) = channel {
        let _ = channel.send(status);
    }
}

extern "C" fn advertised(peripheral: *const c_char, service: *const c_char) -> bool {
    let (peripheral, service) = unsafe { (CStr::from_ptr(peripheral), CStr::from_ptr(service)) };
    let (Ok(peripheral), Ok(service)) = (peripheral.to_str(), service.to_str()) else {
        return false;
    };
    let Some(identity) = decode_ble(&[service.to_owned()]) else {
        return false;
    };
    let radio = radio();
    let state = radio.state.lock().unwrap();
    state.scan.as_ref().is_some_and(|scan| {
        scan.advertisements
            .try_send((identity, peripheral.to_owned()))
            .is_ok()
    })
}

extern "C" fn received(bytes: *const u8, size: usize) {
    let mut state = radio().state.lock().unwrap();
    if size == 0 {
        state.frames.take();
    } else if size <= 512 {
        let bytes = unsafe { std::slice::from_raw_parts(bytes, size) };
        if state
            .frames
            .as_ref()
            .is_some_and(|sender| sender.try_send(bytes.to_vec()).is_err())
        {
            state.frames.take();
        }
    }
}

extern "C" fn completed(operation: u64, success: bool) {
    if let Some(reply) = radio().state.lock().unwrap().operations.remove(&operation) {
        let _ = reply.send(success);
    }
}

#[tauri::command]
pub async fn nearby_bluetooth_enable(
    window: WebviewWindow,
    states: Channel<&'static str>,
) -> Result<(), String> {
    let radio = radio();
    let (advertisements, mut sightings) = mpsc::channel(64);
    let (canceled, cancel) = oneshot::channel();
    let scan_id = Uuid::new_v4().into_string();
    let scope = states.id();
    let owner = window.label().to_owned();
    let status = states.clone();
    {
        let mut state = radio.state.lock().unwrap();
        if state.scan.is_some() {
            return Err("Bluetooth discovery is already active".into());
        }
        state.scan = Some(Scan {
            owner: owner.clone(),
            states,
            advertisements,
            canceled,
        });
    }
    tokio::spawn(async move {
        let report = |active| json!({"scanId": scan_id, "active": active}).to_string();
        if let Err(error) = radio.host.report_ble_scan_state(report(true)).await {
            log::warn!("Starting Bluetooth discovery failed: {error}");
            let _ = status.send("unsupported");
            stop(&owner, Some(scope));
            return;
        }
        if let Err(error) = radio.app.run_on_main_thread(move || {
            if radio
                .state
                .lock()
                .unwrap()
                .scan
                .as_ref()
                .is_some_and(|scan| scan.states.id() == scope)
            {
                unsafe { plain_macos_ble_start(state_changed, advertised, received, completed) };
            }
        }) {
            log::warn!("Starting Bluetooth radio failed: {error}");
            let _ = status.send("unsupported");
            stop(&owner, Some(scope));
        }
        tokio::pin!(cancel);
        loop {
            let (client_id, peripheral) = tokio::select! {
                biased;
                _ = &mut cancel => break,
                sighting = sightings.recv() => match sighting { Some(value) => value, None => break },
            };
            {
                let mut state = radio.state.lock().unwrap();
                if !state.available
                    || !state
                        .scan
                        .as_ref()
                        .is_some_and(|scan| scan.states.id() == scope)
                {
                    continue;
                }
                if state.peers.len() >= 512 && !state.peers.contains_key(&client_id) {
                    continue;
                }
                state.peers.insert(client_id.clone(), peripheral);
            }
            let payload = json!({"scanId": scan_id, "clientId": client_id}).to_string();
            if let Err(error) = radio.host.report_ble_advertisement(payload).await {
                log::debug!("Bluetooth advertisement not accepted: {error}");
            }
        }
        if let Err(error) = radio.host.report_ble_scan_state(report(false)).await {
            log::debug!("Closing Bluetooth discovery failed: {error}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn nearby_bluetooth_stop(window: WebviewWindow, scope: u32) {
    stop(window.label(), Some(scope));
}

#[tauri::command]
pub fn nearby_bluetooth_open_settings(window: WebviewWindow) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    window
        .opener()
        .open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth",
            None::<&str>,
        )
        .map_err(|error| error.to_string())
}

pub fn stop(owner: &str, scope: Option<u32>) {
    let Some(radio) = RADIO.get() else { return };
    let mut state = radio.state.lock().unwrap();
    if !state.scan.as_ref().is_some_and(|scan| {
        scan.owner == owner && scope.is_none_or(|scope| scan.states.id() == scope)
    }) {
        return;
    }
    let scan = state.scan.take().unwrap();
    state.available = false;
    state.peers.clear();
    state.frames.take();
    let _ = scan.canceled.send(());
    drop(state);
    let _ = radio
        .app
        .run_on_main_thread(|| unsafe { plain_macos_ble_stop() });
}

pub fn scan_facts() -> Value {
    json!({"paused": !radio().state.lock().unwrap().available})
}

impl Radio {
    async fn perform(&self, action: impl FnOnce(u64) + Send + 'static) -> Result<(), String> {
        let id = self.operation.fetch_add(1, Ordering::Relaxed);
        let (reply, result) = oneshot::channel();
        self.state.lock().unwrap().operations.insert(id, reply);
        let result = match self.app.run_on_main_thread(move || action(id)) {
            Ok(()) => tokio::time::timeout(Duration::from_secs(6), result)
                .await
                .ok()
                .and_then(Result::ok),
            Err(_) => None,
        };
        self.state.lock().unwrap().operations.remove(&id);
        if result == Some(true) {
            Ok(())
        } else {
            Err("Bluetooth operation failed or timed out".into())
        }
    }

    async fn connect(&'static self, client: &str, characteristic: &str) -> Result<Link, String> {
        let guard = self.link.lock().await;
        let (sender, frames) = mpsc::channel(128);
        let peer = {
            let mut state = self.state.lock().unwrap();
            let peer = state
                .peers
                .get(client)
                .cloned()
                .ok_or("Bluetooth device is no longer nearby")?;
            state.frames = Some(sender);
            peer
        };
        let link = Link {
            radio: self,
            _guard: guard,
            frames,
            assembly: ble::Assembler::default(),
        };
        let service = CString::new(client).map_err(|error| error.to_string())?;
        let peer = CString::new(peer).map_err(|error| error.to_string())?;
        let characteristic = CString::new(characteristic).map_err(|error| error.to_string())?;
        self.perform(move |operation| unsafe {
            plain_macos_ble_connect(
                operation,
                peer.as_ptr(),
                service.as_ptr(),
                characteristic.as_ptr(),
            );
        })
        .await?;
        Ok(link)
    }
}

struct Link {
    radio: &'static Radio,
    _guard: tokio::sync::MutexGuard<'static, ()>,
    frames: mpsc::Receiver<Vec<u8>>,
    assembly: ble::Assembler,
}

impl Link {
    async fn send(&self, request: &[u8]) -> Result<(), String> {
        for sequence in 0.. {
            let Some(frame) =
                ble::frame(request, 1, false, sequence, 20).map_err(|error| error.to_string())?
            else {
                break;
            };
            self.radio
                .perform(move |operation| unsafe {
                    plain_macos_ble_write(operation, frame.as_ptr(), frame.len());
                })
                .await?;
        }
        Ok(())
    }

    async fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, String> {
        match tokio::time::timeout(timeout, async {
            while let Some(frame) = self.frames.recv().await {
                let message = self
                    .assembly
                    .push(&frame)
                    .map_err(|error| error.to_string())?;
                if self.assembly.info() != (1 | (1 << 32)) {
                    return Err("Unexpected Bluetooth response".into());
                }
                if message.is_some() {
                    return Ok(message);
                }
            }
            Err("Bluetooth device disconnected".into())
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Ok(None),
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.radio.state.lock().unwrap().frames.take();
        if let Err(error) = self
            .radio
            .app
            .run_on_main_thread(|| unsafe { plain_macos_ble_disconnect() })
        {
            log::warn!("Closing Bluetooth connection failed: {error}");
        }
    }
}

fn nearby_message(message: Vec<u8>) -> Result<String, String> {
    let payload = ble::nearby_body(&message).map_err(|error| error.to_string())?;
    String::from_utf8(payload.to_vec()).map_err(|error| error.to_string())
}

pub fn transport_available() -> bool {
    radio().state.lock().unwrap().available
}

pub async fn exchange(params: Value, nearby: bool) -> Result<Value, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Request {
        client_id: String,
        request: String,
    }
    let request: Request =
        plain_libs::serde_json::from_value(params).map_err(|error| error.to_string())?;
    let bytes = base64_decode_checked(&request.request).map_err(|error| error.to_string())?;
    let characteristic = if nearby {
        ble::NEARBY_CHAR_UUID
    } else {
        ble::HTTP_CHAR_UUID
    };
    let timeout = Duration::from_secs(if nearby { 15 } else { 115 });
    let response = tokio::time::timeout(timeout, async {
        let mut link = radio().connect(&request.client_id, characteristic).await?;
        link.send(&bytes).await?;
        link.receive(timeout).await
    })
    .await
    .map_err(|_| "Bluetooth exchange timed out")??;
    if nearby {
        Ok(json!({"payload": response.map(nearby_message).transpose()?}))
    } else {
        Ok(json!({"response": response.as_deref().map(base64_encode)}))
    }
}

pub async fn pair(method: &str, params: Value) -> Result<Value, String> {
    let generation = params["generation"]
        .as_str()
        .ok_or("Missing Bluetooth pairing generation")?;
    let mut paired = radio().pairing.lock().await;
    if method == "blePairConnect" {
        if paired.is_some() {
            return Err("Bluetooth pairing is already active".into());
        }
        let client = params["clientId"]
            .as_str()
            .ok_or("Missing Bluetooth device identity")?;
        let link = radio().connect(client, ble::NEARBY_CHAR_UUID).await?;
        *paired = Some((generation.to_owned(), link));
        return Ok(Value::Bool(true));
    }
    if method == "blePairClose" {
        if paired
            .as_ref()
            .is_some_and(|(current, _)| current == generation)
        {
            paired.take();
        }
        return Ok(Value::Bool(true));
    }
    let (_, link) = paired
        .as_mut()
        .filter(|(current, _)| current == generation)
        .ok_or("Bluetooth pairing is no longer active")?;
    match method {
        "blePairSend" => {
            let packet = params["packet"]
                .as_str()
                .ok_or("Missing Bluetooth pairing packet")?;
            let packet = base64_decode_checked(packet).map_err(|error| error.to_string())?;
            link.send(&packet).await?;
            Ok(Value::Bool(true))
        }
        "blePairWait" => {
            let timeout = params["timeoutMs"]
                .as_u64()
                .ok_or("Missing Bluetooth pairing timeout")?
                .clamp(1, 1000);
            let message = link
                .receive(Duration::from_millis(timeout))
                .await?
                .map(nearby_message)
                .transpose()?;
            Ok(json!({"connected": true, "notification": message}))
        }
        _ => Err("Unknown Bluetooth pairing operation".into()),
    }
}
