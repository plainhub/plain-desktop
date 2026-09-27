//! System information: hostname, OS, kernel, CPU, memory, load averages.
//!
//! Replaces the `sysinfo` crate — we read `/proc` directly. The upstream
//! `sysinfo` crate is a 2000+ line cross-platform abstraction that pulls
//! in `rayon` (which pulls in `crossbeam-deque`, `crossbeam-epoch`,
//! `crossbeam-utils`, `either`) just to gather a handful of integers
//! that live in three well-known files on Linux:
//!
//!   * `/proc/meminfo`   — total/free/available/swap memory
//!   * `/proc/stat`      — uptime, idle, boot time
//!   * `/proc/loadavg`   — 1/5/15-minute load averages
//!   * `/proc/cpuinfo`   — model name, core count
//!
//! All of these are small, ASCII, line-oriented, and rarely change
//! format. ~80 lines of std is enough; no platform abstraction needed
//! because plain-nas only runs on Linux (per its install script).

use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(test)]
#[path = "../tests/unit/device_info.rs"]
mod tests;

#[derive(Debug, Serialize, Default)]
pub struct DeviceInfo {
    pub hostname: String,
    pub os: String,
    pub kernel_version: String,
    pub app_version: String,
    pub app_full_version: String,
    pub arch: String,
    pub uptime: i64,
    pub boot_time: i64,
    pub cpu_model: String,
    pub cpu_cores: i32,
    pub cpu_threads: i32,
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,
    pub memory_total_bytes: i64,
    pub memory_free_bytes: i64,
    pub swap_total_bytes: i64,
    pub swap_free_bytes: i64,
    pub swap_used_bytes: i64,
    pub ips: Vec<String>,
    pub nics: Vec<NicInfo>,
    pub model: String,
}

#[derive(Debug, Serialize, Default)]
pub struct NicInfo {
    pub name: String,
    pub mac: String,
    pub speed_rate: i64,
}

pub async fn collect() -> Result<DeviceInfo> {
    let hostname = plain_rs::utils::hostname::get();
    let mem = read_meminfo();
    let load = read_loadavg();
    let cpu = read_cpuinfo();
    let (uptime, boot_time) = read_uptime_and_boottime();

    let ips = std::env::var("PLAIN_NAS_IPS")
        .ok()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Ok(DeviceInfo {
        hostname,
        os: read_os_release(),
        kernel_version: read_kernel_version(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        app_full_version: crate::version::full_version(),
        arch: std::env::consts::ARCH.to_string(),
        uptime,
        boot_time,
        cpu_model: cpu.model,
        cpu_cores: cpu.cores as i32,
        cpu_threads: cpu.threads as i32,
        load1: load.0,
        load5: load.1,
        load15: load.2,
        memory_total_bytes: mem.total,
        memory_free_bytes: mem.free,
        swap_total_bytes: mem.swap_total,
        swap_free_bytes: mem.swap_free,
        swap_used_bytes: mem.swap_total - mem.swap_free,
        ips,
        nics: vec![],
        // Real board name from DMI (NanoPi R5S, N100 box, …); the env
        // override stays for dev/test machines without DMI (Macs, VMs).
        model: {
            let dmi = dmi_model();
            if dmi.is_empty() {
                std::env::var("PLAIN_NAS_MODEL").unwrap_or_default()
            } else {
                dmi
            }
        },
    })
}

// ---------------------------------------------------------------------------
// DeviceStatus (dynamic runtime state, plain-app `DeviceStatus` contract)
// ---------------------------------------------------------------------------

/// Gap between the two /proc/stat samples diffed into CPU usage.
const CPU_SAMPLE_INTERVAL: Duration = Duration::from_millis(200);

/// One thermal zone reading (`/sys/class/thermal/thermal_zone*/`).
#[derive(Debug, Clone, PartialEq)]
pub struct TemperatureData {
    pub label: String,
    pub celsius: f64,
}

/// Raw material for the GraphQL `deviceStatus` resolver (storage usage is
/// resolved separately via `mounts::df_usage`).
#[derive(Debug, Default)]
pub struct DeviceStatusData {
    pub uptime_sec: i64,
    pub cpu_usage: f64,
    pub memory_available: Option<i64>,
    pub temperatures: Vec<TemperatureData>,
}

/// Collects dynamic status from the real filesystem.
pub fn collect_device_status() -> Result<DeviceStatusData> {
    collect_device_status_from(Path::new("/"))
}

/// Root-injectable variant — tests pass a fixture directory holding a
/// synthetic `proc/` and `sys/` tree.
pub fn collect_device_status_from(root: &Path) -> Result<DeviceStatusData> {
    let prev = read_proc_stat(root);
    // A meaningful usage delta needs a gap between the two counter reads.
    std::thread::sleep(CPU_SAMPLE_INTERVAL);
    let cur = read_proc_stat(root);
    let cpu_usage = match (prev, cur) {
        (Some(p), Some(c)) => cpu_usage_percent(&p, &c),
        _ => 0.0,
    };
    Ok(DeviceStatusData {
        uptime_sec: read_uptime_from(root),
        cpu_usage,
        memory_available: read_mem_available(root),
        temperatures: thermal_zones(&root.join("sys/class/thermal")),
    })
}

/// Aggregated CPU tick counters of one sampling instant.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuTimes {
    pub idle: i64,
    pub total: i64,
}

/// Parses the aggregate `cpu  user nice system idle iowait irq softirq steal …`
/// line from /proc/stat. idle = idle + iowait; total = sum of all fields.
/// None for malformed input.
pub fn parse_proc_stat_cpu_line(line: &str) -> Option<CpuTimes> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let values: Vec<i64> = parts.map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some(CpuTimes {
        idle,
        total: values.iter().sum(),
    })
}

/// Diffs two CPU counter samples into a 0-100 usage percentage. Returns 0
/// when the counters did not advance (sampled too fast) instead of dividing
/// by zero; clamps counter resets to the 0-100 range.
pub fn cpu_usage_percent(prev: &CpuTimes, cur: &CpuTimes) -> f64 {
    let idle_delta = cur.idle - prev.idle;
    let total_delta = cur.total - prev.total;
    if total_delta <= 0 {
        return 0.0;
    }
    ((1.0 - idle_delta as f64 / total_delta as f64) * 100.0).clamp(0.0, 100.0)
}

/// Parses `/proc/uptime` content (`"36712.48 12345.67\n"`) into whole
/// seconds of the first field.
pub fn parse_uptime_secs(content: &str) -> i64 {
    content
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as i64)
        .unwrap_or(0)
}

/// Parses `MemAvailable` out of `/proc/meminfo` content (kB → bytes).
/// None when the kernel does not report it (very old kernels).
pub fn parse_mem_available(content: &str) -> Option<i64> {
    for line in content.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "MemAvailable" {
            let mut it = rest.split_whitespace();
            let val = it.next()?.parse::<i64>().ok()?;
            let unit = it.next().unwrap_or("");
            return Some(if unit.eq_ignore_ascii_case("kB") {
                val * 1024
            } else {
                val
            });
        }
    }
    None
}

fn read_proc_stat(root: &Path) -> Option<CpuTimes> {
    let content = std::fs::read_to_string(root.join("proc/stat")).ok()?;
    parse_proc_stat_cpu_line(content.lines().next()?)
}

fn read_uptime_from(root: &Path) -> i64 {
    std::fs::read_to_string(root.join("proc/uptime"))
        .map(|s| parse_uptime_secs(&s))
        .unwrap_or(0)
}

fn read_mem_available(root: &Path) -> Option<i64> {
    std::fs::read_to_string(root.join("proc/meminfo"))
        .ok()
        .and_then(|s| parse_mem_available(&s))
}

/// Reads `<root>/thermal_zone*/` — label from `type`, celsius from `temp`
/// (millidegrees). Zones with missing/invalid readings are skipped; the
/// result is sorted by zone name for stable ordering.
pub fn thermal_zones(class_root: &Path) -> Vec<TemperatureData> {
    let Ok(entries) = std::fs::read_dir(class_root) else {
        return Vec::new();
    };
    let mut zones: Vec<PathBuf> = entries
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
            Some(TemperatureData {
                label,
                celsius: milli / 1000.0,
            })
        })
        .collect()
}

/// Board vendor from `/sys/class/dmi/id/sys_vendor` ("" when absent).
pub fn dmi_manufacturer() -> String {
    dmi_field("sys_vendor")
}

/// Board model from `/sys/class/dmi/id/product_name` ("" when absent).
pub fn dmi_model() -> String {
    dmi_field("product_name")
}

fn dmi_field(name: &str) -> String {
    read_file(&format!("/sys/class/dmi/id/{name}"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[derive(Default)]
struct Mem {
    total: i64,
    free: i64,
    swap_total: i64,
    swap_free: i64,
}

fn read_meminfo() -> Mem {
    let mut m = Mem::default();
    let Some(s) = read_file("/proc/meminfo") else {
        return m;
    };
    // Format: "<Key>:<spaces><value> <unit>\n"  e.g. "MemTotal:       16384000 kB"
    for line in s.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        // value is the second whitespace-separated token (kB on Linux)
        let mut it = rest.split_whitespace();
        let Some(val) = it.next() else { continue };
        let Ok(n) = val.parse::<i64>() else { continue };
        // Unit is usually "kB" — multiply by 1024 to get bytes.
        let unit = it.next().unwrap_or("");
        let bytes = if unit.eq_ignore_ascii_case("kB") {
            n * 1024
        } else {
            n
        };
        match key {
            "MemTotal" => m.total = bytes,
            "MemFree" => m.free = bytes,
            "SwapTotal" => m.swap_total = bytes,
            "SwapFree" => m.swap_free = bytes,
            _ => {}
        }
    }
    m
}

fn read_loadavg() -> (f32, f32, f32) {
    // "0.10 0.15 0.20 1/123 4567" — first three are 1/5/15-minute averages.
    let content = read_file("/proc/loadavg").unwrap_or_default();
    let mut it = content.split_whitespace();
    let a = it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let b = it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let c = it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    (a, b, c)
}

struct Cpu {
    model: String,
    cores: usize,
    threads: usize,
}

fn read_cpuinfo() -> Cpu {
    let mut c = Cpu {
        model: String::new(),
        cores: 0,
        threads: 0,
    };
    let Some(s) = read_file("/proc/cpuinfo") else {
        return c;
    };
    for line in s.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "model name" if c.model.is_empty() => c.model = v.to_string(),
                "cpu cores" => c.cores = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    // "processor" lines = logical CPU count (threads incl. hyperthreading).
    c.threads = s.lines().filter(|l| l.starts_with("processor")).count();
    if c.cores == 0 {
        c.cores = c.threads;
    }
    c
}

fn read_uptime_and_boottime() -> (i64, i64) {
    let uptime = read_file("/proc/uptime")
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as i64)
        .unwrap_or(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (uptime, now - uptime)
}

fn read_kernel_version() -> String {
    read_file("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn read_os_release() -> String {
    // /etc/os-release is the modern source for PRETTY_NAME.
    if let Some(s) = read_file("/etc/os-release") {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                return rest.trim_matches('"').to_string();
            }
        }
    }
    // Fallback: `System::name()` in the old `sysinfo` build returned the
    // long-form OS name. uname(2) is the closest std equivalent.
    String::new()
}

// Touch Duration to keep the import warning away even if the original
// sysinfo::System::new_with_specifics used it (the 200ms sleep was
// there to let sysinfo's two-pass sampling stabilise — we don't need
// that for /proc reads).
#[allow(dead_code)]
const _: Duration = Duration::from_millis(200);
