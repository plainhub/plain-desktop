//! USB volume auto-mounting: mount any present-but-unmounted filesystem by
//! UUID into a stable `/mnt/usbX` slot. Port of Go `internal/storage/mounts.go`
//! (`EnsureMountedUSBVolumes`) + `automount_inhibit.go` + `udev.go`.
//!
//! Slot assignments are persisted (fsuuid → slot) and treated as
//! reservations even while the device is unplugged, so replugged devices
//! keep their historical slot and paths don't churn.
//!
//! The hotplug watcher shells out to
//! `udevadm monitor --udev --subsystem-match=block --property` and
//! debounces bursts into a single reconciliation, exactly like the Go side.

use crate::storage::blockdev::{flatten_devices, run_lsblk};
use crate::prefs::Prefs;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const PLAINNAS_MOUNT_ROOT: &str = "/mnt";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const USB_PREFIX: &str = "usb";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const KEY_SLOT_MAP: &str = "fsuuid_slot_map";

static AUTO_MOUNT_INHIBIT_COUNT: AtomicI32 = AtomicI32::new(0);

/// Temporarily disable `ensure_mounted_usb_volumes` (e.g. while formatting a
/// disk, so reconciliation can't remount what we just unmounted). Re-enables
/// on drop. Go `InhibitAutoMount`.
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

/// Discover filesystems with a UUID, deduped and deterministically ordered.
/// Go `scanFilesystems`.
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
            // Prefer first; lsblk may show the same UUID multiple times in
            // edge cases.
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

/// `mount(8)` supports `-U` for filesystem UUID. Go `mountByUUID`.
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

/// `^/mnt/usb([0-9]+)$` — Go `parseUSBSlot`.
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

/// Smallest positive unused slot (usb1, usb2, ...). Go `nextFreeSlot`.
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
    crate::media::mountinfo::read_mountinfo()
        .map(|es| es.iter().any(|e| e.mount_point == path))
        .unwrap_or(false)
}

/// Clear stacked mount layers at `path` (a hot-unplugged device can leave
/// stale layers behind). Go `unmountAllAt`.
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
            // Busy: lazy-detach so the system drops it when unused.
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

/// One planned mount: which filesystem goes into which (possibly fresh) slot.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct PlannedMount {
    pub fsuuid: String,
    pub slot: usize,
}

/// Pure planning step shared with tests: given discovered filesystems and
/// the persisted slot map, decide the mounts to perform and the merged map
/// to persist. Slot allocation keeps reservations for unplugged devices and
/// never steals an occupied slot.
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
            // Already mounted anywhere; captured above if at /mnt/usbX.
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

    // Merge updates into the persisted map, keeping reservations for
    // currently-unplugged filesystems.
    let mut merged = persisted.clone();
    for (k, v) in mapping {
        merged.0.insert(k, v as i64);
    }
    (planned, merged)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn emit_event(kind: &str, message: &str) {
    if let Some(db) = crate::media::kv::try_get_default() {
        let _ = crate::media::kv::EventLog::new(db).add(kind, message, "");
    }
}

/// Mount any present-but-not-mounted filesystem into `/mnt/usbX`. Go
/// `EnsureMountedUSBVolumes`. No-op when inhibited or off Linux.
#[cfg_attr(not(target_os = "linux"), allow(clippy::needless_return))]
pub fn ensure_mounted_usb_volumes(prefs: &Prefs) -> Result<()> {
    if auto_mount_inhibited() {
        return Ok(());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = prefs;
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
            // Always clear /mnt/usbX before mounting: Linux can keep older
            // layers from a hot-unplugged device.
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

        store_slot_map(prefs, &merged)
    }
}

/// Run `ensure_mounted_usb_volumes` on a worker thread, bounded by
/// `timeout` (Go wraps the call in `context.WithTimeout`).
pub fn ensure_mounted_with_timeout(prefs: &Arc<Prefs>, timeout: Duration) {
    let prefs = prefs.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let _ = ensure_mounted_usb_volumes(&prefs);
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(timeout);
}

/// Go `shouldTriggerHotplug`.
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

/// Event-driven hotplug reconciliation via `udevadm monitor`. Runs until the
/// process exits. Go `RunAutoMountWatcher` (700ms debounce).
pub fn run_automount_watcher() {
    if which("udevadm").is_none() {
        log::error!("storage hotplug: udevadm not found");
        return;
    }

    let trigger: Arc<Mutex<Option<std::time::Instant>>> = Arc::new(Mutex::new(None));
    let t = trigger.clone();
    std::thread::spawn(move || {
        // Single consumer: udevadm emits bursts during hotplug; each event
        // pushes the deadline out so one reconciliation covers the burst.
        loop {
            let deadline = {
                let mut g = t.lock().unwrap();
                if g.is_none() {
                    *g = Some(std::time::Instant::now() + Duration::from_millis(700));
                }
                *g
            };
            match deadline {
                Some(d) => {
                    let now = std::time::Instant::now();
                    if now >= d {
                        *t.lock().unwrap() = None;
                        if let Some(prefs) = crate::prefs::try_get_default() {
                            ensure_mounted_with_timeout(&prefs, Duration::from_secs(30));
                        }
                    } else {
                        std::thread::sleep(d - now);
                    }
                }
                None => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    });

    let mut child = match Command::new("udevadm")
        .args(["monitor", "--udev", "--subsystem-match=block", "--property"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("storage hotplug: start udevadm failed: {e}");
            return;
        }
    };
    let stdout = child.stdout.take().expect("udevadm stdout");
    let reader = BufReader::new(stdout);
    let mut props: HashMap<String, String> = HashMap::new();
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            if should_trigger_hotplug(&props) {
                // Nudge the debounce deadline (create or push out).
                let mut g = trigger.lock().unwrap();
                *g = Some(std::time::Instant::now() + Duration::from_millis(700));
            }
            props.clear();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            props.insert(k.to_string(), v.to_string());
        }
    }
    let _ = child.wait();
}

fn which(prog: &str) -> Option<()> {
    let ok = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {prog}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok.then_some(())
}

#[cfg(test)]
#[path = "../../tests/unit/storage/automount.rs"]
mod tests;
