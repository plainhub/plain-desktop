use async_graphql::{Context, Object};
use serde_json::json;
use std::sync::Arc;

use super::super::context::{AppCtx, WS_DEVICE_NAME_UPDATED, WsEvent};
use super::types::{App, AudioPlayback, Capability, DeviceInfo, DevicePlatform, DeviceStatus, Sim, Temperature};
use crate::local::enums::{AppChannelType, DeviceType};

#[cfg(test)]
#[path = "../../../../tests/unit/local/graphql/schema/app.rs"]
mod tests;

#[derive(Default)]
pub struct AppQuery;

#[Object]
impl AppQuery {
    async fn app(&self, ctx: &Context<'_>) -> App {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        App {
            client_id: c.identity.client_id.clone(),
            url_token: c.token.clone(),
            http_port: c.port.load(std::sync::atomic::Ordering::Relaxed) as i32,
            https_port: c.https_port.load(std::sync::atomic::Ordering::Relaxed) as i32,
            // Matches Android `context.appDir()` — the parent directory of the
            // content-addressable `{hash[0..1]}/{hash[2..3]}/` sharded layout.
            // The web client uses `appDir` to build `fid:` paths (see
            // `getFinalPath` in `lib/api/file.ts`), so it must point at
            // `{data_dir}/files` to match where the local file_server
            // (`/fs` route) reads from.
            app_dir: c.data_dir.join("files").to_string_lossy().into_owned(),
            device_name: c.device_name.read().unwrap().clone(),
            device_type: DeviceType::Computer,
            // The desktop local-mode surface: image editor and the shared
            // web document viewer (chat/files/media are base features).
            capabilities: vec![Capability::DocPreview, Capability::ImageEditor],
            build_channel: AppChannelType::Github,
            permissions: vec![],
            downloads_dir: String::new(),
            developer_mode: false,
            debug: cfg!(debug_assertions),
        }
    }

    async fn device_info(&self, ctx: &Context<'_>) -> DeviceInfo {
        use std::env::consts;
        use sysinfo::System;
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let device_name = c.device_name.read().unwrap().clone();

        let mut sys = System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();
        let cpu_model = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|s| !s.is_empty());
        let total_memory = sys.total_memory() as i64;
        let (total_storage, _) = volume_for(&c.data_dir);

        let os_name = System::name().unwrap_or_default();
        let os_version = System::long_os_version().unwrap_or_default();
        let kernel_version = System::kernel_version().unwrap_or_default();

        let model = hw_model();
        let manufacturer = manufacturer();
        let language = system_language();

        DeviceInfo {
            name: device_name,
            platform: current_platform(),
            manufacturer,
            model,
            os_name,
            os_version,
            kernel_version,
            app_version: c.handle.package_info().version.to_string(),
            app_build_number: String::new(),
            language,
            cpu_arch: consts::ARCH.to_string(),
            cpu_model,
            total_memory,
            total_storage: total_storage as i64,
            display: None,
            android: None,
        }
    }

    async fn sims(&self) -> Vec<Sim> {
        vec![]
    }

    /// Inert player state — the desktop backend has no audio engine.
    async fn audio_playback(&self) -> AudioPlayback {
        AudioPlayback {
            mode: "REPEAT".to_string(),
            current_path: String::new(),
            is_playing: false,
            position_ms: 0,
        }
    }

    async fn device_status(&self, ctx: &Context<'_>) -> DeviceStatus {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let data_dir = c.data_dir.clone();
        tokio::task::spawn_blocking(move || collect_device_status(&data_dir))
            .await
            .unwrap_or_default()
    }
}

#[derive(Default)]
pub struct AppMutation;

#[Object]
impl AppMutation {
    async fn update_device_name(&self, ctx: &Context<'_>, name: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        // Updates the shared name AND republishes the mDNS service (goodbye
        // for the old instance) so peers see the new name right away.
        c.discover_manager.apply_device_rename(&name);
        crate::prefs::set_device_name(&c.handle, &name);
        let _ = c.event_tx.send(WsEvent {
            event_type: WS_DEVICE_NAME_UPDATED,
            payload: json!(name).to_string(),
        });
        true
    }
}

// ── System info helpers ───────────────────────────────────────────────────────

fn current_platform() -> DevicePlatform {
    #[cfg(target_os = "macos")]
    {
        return DevicePlatform::Macos;
    }
    #[cfg(target_os = "windows")]
    {
        return DevicePlatform::Windows;
    }
    #[cfg(target_os = "linux")]
    {
        return DevicePlatform::Linux;
    }
    #[allow(unreachable_code)]
    DevicePlatform::Linux
}

fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn hw_model() -> String {
    #[cfg(target_os = "macos")]
    if let Some(m) = run_cmd("sysctl", &["-n", "hw.model"]) {
        return m;
    }
    #[cfg(target_os = "linux")]
    if let Ok(s) = std::fs::read_to_string("/sys/class/dmi/id/product_name") {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    sysinfo::System::host_name().unwrap_or_default()
}

fn manufacturer() -> String {
    #[cfg(target_os = "macos")]
    {
        return "Apple".to_string();
    }
    #[cfg(target_os = "linux")]
    if let Ok(s) = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor") {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(s) = run_cmd("wmic", &["csproduct", "get", "vendor", "/value"]) {
        if let Some(v) = s.lines().find(|l| l.starts_with("Vendor=")) {
            return v.trim_start_matches("Vendor=").to_string();
        }
    }
    #[allow(unreachable_code)]
    String::new()
}

fn system_language() -> String {
    #[cfg(target_os = "macos")]
    if let Some(locale) = run_cmd("defaults", &["read", "-g", "AppleLocale"]) {
        let lang = locale.split('_').next().unwrap_or("").to_string();
        if !lang.is_empty() && lang != "C" {
            return lang;
        }
    }
    std::env::var("LANG")
        .unwrap_or_default()
        .split('.')
        .next()
        .and_then(|s| s.split('_').next().map(|l| l.to_string()))
        .filter(|s| !s.is_empty() && s != "C")
        .unwrap_or_default()
}

// ── DeviceStatus collection ───────────────────────────────────────────────────

fn collect_device_status(data_dir: &std::path::Path) -> DeviceStatus {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    // sysinfo needs a minimum interval between CPU samples for a meaningful
    // delta; refreshing twice in a row reports 0%.
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();

    let (battery_level, charging) = battery_status();
    let (_, storage_available) = volume_for(data_dir);

    DeviceStatus {
        uptime_sec: System::uptime() as i64,
        battery_level,
        charging,
        temperatures: platform_temperatures(),
        cpu_usage: (sys.global_cpu_usage() as f64).clamp(0.0, 100.0),
        memory_available: Some(sys.available_memory() as i64),
        storage_available: storage_available as i64,
    }
}

/// (level 0-100, charging) from the platform power API; (None, false) when
/// the device has no battery (desktops) or the level is unknown.
fn battery_status() -> (Option<i32>, bool) {
    #[cfg(target_os = "macos")]
    {
        if let Some(out) = run_cmd("pmset", &["-g", "batt"]) {
            return parse_pmset_battery(&out)
                .map(|(level, charging)| (Some(level), charging))
                .unwrap_or((None, false));
        }
        return (None, false);
    }
    #[cfg(target_os = "windows")]
    {
        return win_power::read()
            .map(|(level, charging)| (Some(level), charging))
            .unwrap_or((None, false));
    }
    #[allow(unreachable_code)]
    (None, false)
}

/// Parses `pmset -g batt` output into (level, charging).
///
/// ```text
/// Now drawing from 'AC Power'
///  -InternalBattery-0 (id=...) 87%; discharging; 3:28 remaining
/// ```
///
/// Charging is true only for an actively charging battery — "charged" and
/// "finishing charge" (full while plugged in) are false per the contract.
pub(crate) fn parse_pmset_battery(out: &str) -> Option<(i32, bool)> {
    if !out.contains("InternalBattery") {
        return None;
    }
    let mut level = None;
    let mut charging = false;
    for line in out.lines() {
        if !line.contains("InternalBattery") {
            continue;
        }
        if let Some(pct_part) = line.split('%').next()
            && let Some(pct_str) = pct_part.split_whitespace().last()
            && let Ok(pct) = pct_str.parse::<i32>()
        {
            level = Some(pct);
        }
        // "discharging" contains the substring "charging" — exclude it.
        if line.contains("charging") && !line.contains("discharging") {
            charging = true;
        }
    }
    level.map(|l| (l, charging))
}

/// Maps SYSTEM_POWER_STATUS fields to (level, charging).
/// BatteryFlag 128 = no system battery; 8 = charging; 255 = unknown level.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn win_battery_from(battery_flag: u8, battery_life_percent: u8) -> Option<(i32, bool)> {
    if battery_flag & 128 != 0 || battery_life_percent == 255 || battery_life_percent > 100 {
        return None;
    }
    Some((battery_life_percent as i32, battery_flag & 8 != 0))
}

/// Raw kernel32 FFI for GetSystemPowerStatus — one call, avoids pulling in a
/// windows-sys dependency for a single function.
#[cfg(target_os = "windows")]
mod win_power {
    #[repr(C)]
    #[derive(Default)]
    struct SystemPowerStatus {
        ac_line_status: u8,
        battery_flag: u8,
        battery_life_percent: u8,
        reserved1: u8,
        battery_life_time: u32,
        battery_full_life_time: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
    }

    pub fn read() -> Option<(i32, bool)> {
        let mut s = SystemPowerStatus::default();
        // SAFETY: single call passing a valid out-pointer to a Win32 API.
        if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
            return None;
        }
        super::win_battery_from(s.battery_flag, s.battery_life_percent)
    }
}

fn platform_temperatures() -> Vec<Temperature> {
    #[cfg(target_os = "linux")]
    return thermal_zones(std::path::Path::new("/sys/class/thermal"));
    #[allow(unreachable_code)]
    Vec::new()
}

/// Reads `<root>/thermal_zone*/` — label from `type`, celsius from `temp`
/// (millidegrees). Zones with missing/invalid readings are skipped.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn thermal_zones(root: &std::path::Path) -> Vec<Temperature> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut zones: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("thermal_zone"))
        })
        .collect();
    zones.sort();
    zones
        .into_iter()
        .filter_map(|zone| {
            let label = std::fs::read_to_string(zone.join("type"))
                .unwrap_or_default()
                .trim()
                .to_string();
            let milli: f64 = std::fs::read_to_string(zone.join("temp"))
                .ok()?
                .trim()
                .parse()
                .ok()?;
            Some(Temperature {
                label,
                celsius: milli / 1000.0,
            })
        })
        .collect()
}

/// The mount point (from the candidate list) that best backs `path`: the
/// longest ancestor prefix. `/` matches everything, so a more specific mount
/// always wins.
pub(crate) fn longest_prefix_mount<'a>(
    mounts: &[&'a std::path::Path],
    path: &std::path::Path,
) -> Option<&'a std::path::Path> {
    mounts
        .iter()
        .copied()
        .filter(|m| path.starts_with(m))
        .max_by_key(|m| m.as_os_str().len())
}

/// (total, available) bytes of the volume backing `path`.
fn volume_for(path: &std::path::Path) -> (u64, u64) {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let mounts: Vec<&std::path::Path> = disks.list().iter().map(|d| d.mount_point()).collect();
    match longest_prefix_mount(&mounts, path) {
        Some(mount) => match disks.list().iter().find(|d| d.mount_point() == mount) {
            Some(d) => (d.total_space(), d.available_space()),
            None => (0, 0),
        },
        None => (0, 0),
    }
}
