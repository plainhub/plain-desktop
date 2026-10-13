use crate::platform::linux::disks::blockdev::flatten_devices;
use crate::platform::linux::disks::blockdev::run_lsblk;
use crate::prefs::Prefs;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const PLAINNAS_MOUNT_ROOT: &str = "/mnt";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const USB_PREFIX: &str = "usb";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const KEY_SLOT_MAP: &str = "fsuuid_slot_map";

static AUTO_MOUNT_INHIBIT_COUNT: AtomicI32 = AtomicI32::new(0);

pub struct AutoMountInhibit;

pub fn inhibit() -> AutoMountInhibit {
    AUTO_MOUNT_INHIBIT_COUNT.fetch_add(1, Ordering::SeqCst);
    AutoMountInhibit
}

impl Drop for AutoMountInhibit {
    fn drop(&mut self) {
        AUTO_MOUNT_INHIBIT_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

fn auto_mount_inhibited() -> bool {
    AUTO_MOUNT_INHIBIT_COUNT.load(Ordering::SeqCst) > 0
}

#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct BlockFs {
    pub path: String,
    pub fsuuid: String,
    pub fstype: String,
    pub size_bytes: u64,
    pub mountpoint: String,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn scan_filesystems() -> Result<Vec<BlockFs>> {
    let devs = run_lsblk(&[
        "NAME",
        "PATH",
        "TYPE",
        "FSTYPE",
        "UUID",
        "SIZE",
        "MOUNTPOINT",
    ])?;
    let skip_fs_types = ["swap", "crypto_LUKS", "LVM2_member"];
    let mut items: Vec<BlockFs> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for d in flatten_devices(&devs) {
        let uuid = d.uuid.trim();
        let fstype = d.fstype.trim();
        let path = d.path.trim();
        if uuid.is_empty() || fstype.is_empty() || path.is_empty() {
            continue;
        }
        if skip_fs_types.contains(&fstype) {
            continue;
        }
        if !seen.insert(uuid.to_string()) {
            continue;
        }
        items.push(BlockFs {
            path: path.to_string(),
            fsuuid: uuid.to_string(),
            fstype: fstype.to_string(),
            size_bytes: d.size.unwrap_or(0).max(0) as u64,
            mountpoint: d.mountpoint.as_deref().unwrap_or("").trim().to_string(),
        });
    }
    items.sort_by(|a, b| a.fsuuid.cmp(&b.fsuuid));
    Ok(items)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn mount_by_uuid(fs_uuid: &str, target: &str) -> Result<()> {
    if fs_uuid.is_empty() {
        return Err(anyhow!("empty uuid"));
    }
    let out = Command::new("mount")
        .arg("-U")
        .arg(fs_uuid)
        .arg(target)
        .output()
        .map_err(|e| anyhow!("mount spawn: {e}"))?;
    if !out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        let s = s.trim();
        return Err(anyhow!("{}", if s.is_empty() { "mount failed" } else { s }));
    }
    Ok(())
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_usb_slot(mountpoint: &str) -> Option<usize> {
    let rest = mountpoint
        .strip_prefix(PLAINNAS_MOUNT_ROOT)?
        .strip_prefix('/')?;
    let num = rest.strip_prefix(USB_PREFIX)?;
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    num.parse::<usize>().ok()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn next_free_slot(used: &HashSet<usize>) -> usize {
    let mut i = 1;
    while used.contains(&i) {
        i += 1;
    }
    i
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_mountpoint(path: &str) -> bool {
    crate::platform::mountinfo::read_mountinfo()
        .map(|es| es.iter().any(|e| e.mount_point == path))
        .unwrap_or(false)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[cfg(target_os = "linux")]
fn unmount_all_at(path: &str) -> Result<()> {
    use nix::mount::{MntFlags, umount, umount2};
    const MAX_LAYERS: usize = 32;
    for _ in 0..MAX_LAYERS {
        if !is_mountpoint(path) {
            return Ok(());
        }
        if umount(path).is_err() {
            if umount2(path, MntFlags::MNT_DETACH).is_err() {
                return umount(path).map_err(|e| anyhow!("{e}"));
            }
        }
    }
    Err(anyhow!("too many mount layers at {path}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct SlotMap(BTreeMap<String, i64>);

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn get_slot_map(prefs: &Prefs) -> SlotMap {
    prefs
        .get::<SlotMap>(KEY_SLOT_MAP)
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn store_slot_map(prefs: &Prefs, m: &SlotMap) -> Result<()> {
    prefs
        .set(KEY_SLOT_MAP, m.clone())
        .map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct PlannedMount {
    pub fsuuid: String,
    pub slot: usize,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn plan_mounts(filesystems: &[BlockFs], persisted: &SlotMap) -> (Vec<PlannedMount>, SlotMap) {
    let mut mapping: HashMap<String, usize> = HashMap::new();
    let mut occupied_slots: HashMap<usize, String> = HashMap::new();
    let mut used_slots: HashSet<usize> = persisted
        .0
        .values()
        .filter(|v| **v > 0)
        .map(|v| *v as usize)
        .collect();

    for fs in filesystems {
        if fs.fsuuid.is_empty() {
            continue;
        }
        if let Some(slot) = parse_usb_slot(&fs.mountpoint) {
            mapping.insert(fs.fsuuid.clone(), slot);
            occupied_slots.insert(slot, fs.fsuuid.clone());
            used_slots.insert(slot);
        }
    }

    let mut planned = Vec::new();
    for fs in filesystems {
        if fs.fsuuid.is_empty() || !fs.mountpoint.is_empty() {
            continue;
        }

        let mut slot = persisted
            .0
            .get(&fs.fsuuid)
            .copied()
            .filter(|v| *v > 0)
            .map(|v| v as usize);
        if let Some(s) = slot
            && let Some(other) = occupied_slots.get(&s)
            && other != &fs.fsuuid
        {
            slot = None;
        }
        let slot = slot.unwrap_or_else(|| next_free_slot(&used_slots));
        used_slots.insert(slot);
        mapping.insert(fs.fsuuid.clone(), slot);
        occupied_slots.insert(slot, fs.fsuuid.clone());
        planned.push(PlannedMount {
            fsuuid: fs.fsuuid.clone(),
            slot,
        });
    }

    let mut merged = persisted.clone();
    for (k, v) in mapping {
        merged.0.insert(k, v as i64);
    }
    (planned, merged)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub type EventSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub fn ensure_mounted_usb_volumes(prefs: &Prefs, emit_event: &EventSink) -> Result<()> {
    if auto_mount_inhibited() {
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (prefs, emit_event);
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::create_dir_all(PLAINNAS_MOUNT_ROOT)?;

        let filesystems = scan_filesystems()?;
        let persisted = get_slot_map(prefs);
        let (planned, merged) = plan_mounts(&filesystems, &persisted);

        let by_uuid: HashMap<&str, &BlockFs> = filesystems
            .iter()
            .map(|fs| (fs.fsuuid.as_str(), fs))
            .collect();
        for pm in &planned {
            let Some(fs) = by_uuid.get(pm.fsuuid.as_str()) else {
                continue;
            };
            let target = format!("{PLAINNAS_MOUNT_ROOT}/{USB_PREFIX}{}", pm.slot);
            if let Err(e) = std::fs::create_dir_all(&target) {
                log::error!("mount: mkdir {target} failed: {e}");
                emit_event("mount_failed", &format!("mkdir {target}: {e}"));
                continue;
            }
            if let Err(e) = unmount_all_at(&target) {
                log::error!("mount: cleanup {target} failed: {e}");
                emit_event("mount_failed", &format!("cleanup {target}: {e}"));
                continue;
            }
            if let Err(e) = mount_by_uuid(&fs.fsuuid, &target) {
                log::error!("mount: UUID {} -> {target} failed: {e}", fs.fsuuid);
                emit_event(
                    "mount_failed",
                    &format!("UUID {} -> {target}: {e}", fs.fsuuid),
                );
                continue;
            }
            log::info!("mounted UUID {} at {target}", fs.fsuuid);
            emit_event("mount", &format!("mounted UUID {} at {target}", fs.fsuuid));
        }

        store_slot_map(prefs, &merged)?;
        emit_event("mounts_changed", "");
        Ok(())
    }
}

fn should_trigger_hotplug(props: &HashMap<String, String>) -> bool {
    if props.is_empty() {
        return false;
    }
    if let Some(ss) = props.get("SUBSYSTEM")
        && !ss.is_empty()
        && ss != "block"
    {
        return false;
    }
    match props.get("ACTION").map(String::as_str) {
        Some("add") | Some("remove") | Some("change") => {}
        _ => return false,
    }
    match props.get("DEVTYPE").map(String::as_str).unwrap_or("") {
        "" | "disk" | "partition" => {}
        _ => return false,
    }
    true
}

pub struct Watcher {
    stop: tokio::sync::watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
}

impl Watcher {
    pub fn start(prefs: Arc<Prefs>, events: EventSink) -> Self {
        let (stop, receiver) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(watch(prefs, events, receiver));
        Self {
            stop,
            task: Some(task),
        }
    }

    pub async fn wait(&mut self) -> Result<()> {
        let result = match self.task.as_mut() {
            Some(task) => task.await?,
            None => return Ok(()),
        };
        self.task.take();
        result
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.stop.send(true);
        if let Some(task) = self.task.take() {
            task.await??;
        }
        Ok(())
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn watch(
    prefs: Arc<Prefs>,
    events: EventSink,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    if !cfg!(target_os = "linux") {
        let mut stop = stop;
        if !*stop.borrow() {
            let _ = stop.changed().await;
        }
        return Ok(());
    }
    let child = tokio::process::Command::new("udevadm")
        .args(["monitor", "--udev", "--subsystem-match=block", "--property"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    watch_child(child, prefs, events, stop).await
}

async fn watch_child(
    mut child: tokio::process::Child,
    prefs: Arc<Prefs>,
    events: EventSink,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    use tokio::io::AsyncBufReadExt;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("udevadm stdout unavailable"))?;
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut props = HashMap::new();
    let mut deadline = None;
    let result = loop {
        if *stop.borrow() {
            break Ok(());
        }
        tokio::select! {
            biased;
            _ = stop.changed() => break Ok(()),
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        let line = line.trim();
                        if line.is_empty() {
                            if should_trigger_hotplug(&props) {
                                deadline = Some(tokio::time::Instant::now() + Duration::from_millis(700));
                            }
                            props.clear();
                        } else if let Some((key, value)) = line.split_once('=') {
                            props.insert(key.to_string(), value.to_string());
                        }
                    }
                    Ok(None) => break Err(anyhow!("udevadm monitor stopped")),
                    Err(error) => break Err(error.into()),
                }
            }
            _ = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                deadline = None;
                let prefs = prefs.clone();
                let events = events.clone();
                match tokio::task::spawn_blocking(move || ensure_mounted_usb_volumes(&prefs, &events)).await {
                    Ok(Ok(())) => {},
                    Ok(Err(error)) => log::error!("storage mount ensure failed: {error}"),
                    Err(error) => break Err(error.into()),
                }
            }
        }
    };
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

#[cfg(test)]
#[path = "../../../../tests/unit/platform/linux/mounts/automount.rs"]
mod tests;
