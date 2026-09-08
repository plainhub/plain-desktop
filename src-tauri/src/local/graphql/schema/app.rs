use async_graphql::{Context, Object};
use serde_json::json;
use std::sync::Arc;

use super::super::context::{AppCtx, WsEvent, WS_DEVICE_NAME_UPDATED};
use super::types::{
    App, BatteryInfo, DesktopDeviceInfo, DeviceInfo, DevicePlatform, Sim,
};
#[cfg(target_os = "macos")]
use super::types::{BatteryHealth, BatteryPlugged, BatteryStatus};
use crate::local::enums::{AppChannelType, DeviceType};

#[derive(Default)]
pub struct AppQuery;

#[Object]
impl AppQuery {
    async fn app(&self, ctx: &Context<'_>) -> App {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        App {
            client_id: c.identity.client_id.clone(),
            usb_connected: false,
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
            battery: String::new(),
            app_version: String::new(),
            os_version: String::new(),
            channel: AppChannelType::Github,
            permissions: vec![],
            audios: vec![],
            audio_current: String::new(),
            audio_mode: String::new(),
            sdcard_path: String::new(),
            usb_disk_paths: vec![],
            internal_storage_path: String::new(),
            downloads_dir: String::new(),
            developer_mode: false,
            favorite_folders: vec![],
            debug: cfg!(debug_assertions),
        }
    }

    async fn device_info(&self, ctx: &Context<'_>) -> DeviceInfo {
        use std::env::consts;
        use sysinfo::System;
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let device_name = c.device_name.read().unwrap().clone();

        let sys = System::new_all();
        let cpu_model = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_default();
        let total_memory = sys.total_memory() as i64;

        let hostname = System::host_name().unwrap_or_default();
        let os_name = System::name().unwrap_or_default();
        let os_version = System::long_os_version().unwrap_or_default();
        let kernel_version = System::kernel_version().unwrap_or_default();
        let uptime_ms = (System::uptime() * 1000) as i64;

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
            uptime: uptime_ms,
            cpu_arch: consts::ARCH.to_string(),
            total_memory,
            total_storage: 0,
            display: None,
            android: None,
            desktop: Some(DesktopDeviceInfo {
                hostname,
                cpu_model,
                gpu_model: String::new(),
                desktop_environment: desktop_environment(),
                window_manager: String::new(),
            }),
        }
    }

    async fn sims(&self) -> Vec<Sim> {
        vec![]
    }

    async fn battery(&self) -> Option<BatteryInfo> {
        battery_info()
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

fn desktop_environment() -> String {
    std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .unwrap_or_default()
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

fn battery_info() -> Option<BatteryInfo> {
    #[cfg(target_os = "macos")]
    return macos_battery();
    #[allow(unreachable_code)]
    None
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

#[cfg(target_os = "macos")]
fn macos_battery() -> Option<BatteryInfo> {
    // pmset -g batt example output:
    //   Now drawing from 'Battery Power'
    //    -InternalBattery-0 (id=...)	87%; discharging; 3:28 remaining
    let out = run_cmd("pmset", &["-g", "batt"])?;
    // If no InternalBattery line, this is a desktop — no battery.
    if !out.contains("InternalBattery") {
        return None;
    }

    let mut level = 100i32;
    let mut plugged = BatteryPlugged::Ac;
    let mut status = BatteryStatus::Full;

    for line in out.lines() {
        if line.contains("drawing from") && line.contains("Battery Power") {
            plugged = BatteryPlugged::Unplugged;
        }
        if line.contains("InternalBattery") {
            // Extract percentage: "87%"
            if let Some(pct_part) = line.split('%').next()
                && let Some(pct_str) = pct_part.split_whitespace().last()
            {
                level = pct_str.parse().unwrap_or(100);
            }
            if line.contains("discharging") {
                status = BatteryStatus::Discharging;
            } else if line.contains("charging") {
                status = BatteryStatus::Charging;
            } else if line.contains("charged") || line.contains("finishing") {
                status = BatteryStatus::Full;
            }
        }
    }

    Some(BatteryInfo {
        level,
        voltage: 0,
        health: BatteryHealth::Good,
        plugged,
        temperature: 0.0,
        status,
        technology: "Li-ion".to_string(),
        capacity: 0,
    })
}
