//! Unit tests for `src/device_info.rs` — the /proc /sys readers and the
//! pure parsers behind the `deviceInfo` / `deviceStatus` GraphQL surface.
//! All fixtures are synthetic files under a temp root (the injection seam),
//! so nothing depends on the host machine's real /proc.
use super::*;
use std::fs;

fn fixture_root(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("plain-nas-devinfo-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn parse_proc_stat_cpu_line_sums_and_adds_iowait_to_idle() {
    let t = parse_proc_stat_cpu_line("cpu  74608 2520 24433 1117073 6176 4054 0 0 0 0").unwrap();
    assert_eq!(t.idle, 1117073 + 6176);
    assert_eq!(t.total, 74608 + 2520 + 24433 + 1117073 + 6176 + 4054);

    let min = parse_proc_stat_cpu_line("cpu  1 2 3 4").unwrap();
    assert_eq!(min.idle, 4);
    assert_eq!(min.total, 10);
}

#[test]
fn parse_proc_stat_cpu_line_rejects_malformed() {
    assert!(parse_proc_stat_cpu_line("").is_none());
    assert!(parse_proc_stat_cpu_line("meminfo: 42").is_none());
    assert!(parse_proc_stat_cpu_line("cpux 1 2 3 4").is_none());
    assert!(parse_proc_stat_cpu_line("cpu  1 2 3").is_none());
    assert!(parse_proc_stat_cpu_line("cpu  1 2 3 x 5").is_none());
}

#[test]
fn cpu_usage_percent_diffs_samples() {
    let prev = CpuTimes {
        idle: 100,
        total: 400,
    };
    let cur = CpuTimes {
        idle: 125,
        total: 500,
    };
    assert_eq!(cpu_usage_percent(&prev, &cur), 75.0);
}

#[test]
fn cpu_usage_percent_never_divides_by_zero_or_underflows() {
    // Counters did not advance → 0, not NaN/Inf.
    let same = CpuTimes {
        idle: 100,
        total: 400,
    };
    assert_eq!(cpu_usage_percent(&same, &same), 0.0);
    // Counter reset (smaller totals) → 0, never negative.
    assert_eq!(
        cpu_usage_percent(
            &CpuTimes {
                idle: 500,
                total: 900
            },
            &CpuTimes {
                idle: 10,
                total: 100
            }
        ),
        0.0
    );
}

#[test]
fn cpu_usage_percent_clamps_idle_reset_to_hundred() {
    let prev = CpuTimes {
        idle: 50,
        total: 100,
    };
    let cur = CpuTimes {
        idle: 10,
        total: 110,
    };
    assert_eq!(cpu_usage_percent(&prev, &cur), 100.0);
}

#[test]
fn parse_uptime_secs_takes_first_field_in_seconds() {
    assert_eq!(parse_uptime_secs("36712.48 12345.67\n"), 36712);
    assert_eq!(parse_uptime_secs("42.99 1.00"), 42);
    assert_eq!(parse_uptime_secs(""), 0);
    assert_eq!(parse_uptime_secs("garbage"), 0);
}

#[test]
fn parse_mem_available_converts_kb_to_bytes() {
    let content =
        "MemTotal:       16384000 kB\nMemAvailable:    8192000 kB\nMemFree:        1024000 kB\n";
    assert_eq!(parse_mem_available(content), Some(8192000 * 1024));
    // Absent key → None; no unit → raw value.
    assert_eq!(parse_mem_available("MemTotal: 100 kB\n"), None);
    assert_eq!(parse_mem_available("MemAvailable: 4096\n"), Some(4096));
    assert_eq!(parse_mem_available(""), None);
}

#[test]
fn thermal_zones_read_type_and_millidegrees_and_skip_broken() {
    let root = fixture_root("thermal");
    let class = root.join("sys/class/thermal");
    for (zone, label, milli) in [
        ("zone1", "cpu-thermal", "45000"),
        ("zone2", "gpu-thermal", "38500"),
    ] {
        let dir = class.join(format!("thermal_{zone}"));
        write(&dir.join("type"), label);
        write(&dir.join("temp"), milli);
    }
    let broken = class.join("thermal_zone3");
    write(&broken.join("type"), "broken");
    write(&broken.join("temp"), "not-a-number");
    write(&class.join("not_a_zone").join("type"), "x");

    let temps = thermal_zones(&class);
    assert_eq!(
        temps,
        vec![
            TemperatureData {
                label: "cpu-thermal".into(),
                celsius: 45.0
            },
            TemperatureData {
                label: "gpu-thermal".into(),
                celsius: 38.5
            },
        ]
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn thermal_zones_missing_root_is_empty() {
    assert!(thermal_zones(Path::new("/nonexistent-plain-nas-test")).is_empty());
}

#[test]
fn collect_device_status_from_reads_the_whole_synthetic_tree() {
    let root = fixture_root("status");
    write(&root.join("proc/uptime"), "36712.48 12345.67\n");
    write(
        &root.join("proc/stat"),
        "cpu  74608 2520 24433 1117073 6176 4054 0 0 0 0\ncpu0 1 2 3 4 5 6 7 8 9\n",
    );
    write(
        &root.join("proc/meminfo"),
        "MemTotal: 100 kB\nMemAvailable: 4096 kB\n",
    );
    let thermal = root.join("sys/class/thermal/thermal_zone1");
    write(&thermal.join("type"), "soc-thermal\n");
    write(&thermal.join("temp"), "52000\n");

    let st = collect_device_status_from(&root).unwrap();
    assert_eq!(st.uptime_sec, 36712);
    assert_eq!(st.memory_available, Some(4096 * 1024));
    assert_eq!(
        st.temperatures,
        vec![TemperatureData {
            label: "soc-thermal".into(),
            celsius: 52.0
        }]
    );
    // The fixture /proc/stat never changes between the two samples, so the
    // diff must saturate at 0 — never NaN and never a panic.
    assert_eq!(st.cpu_usage, 0.0);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn collect_device_status_from_tolerates_missing_proc() {
    let root = fixture_root("empty");
    let st = collect_device_status_from(&root).unwrap();
    assert_eq!(st.uptime_sec, 0);
    assert_eq!(st.cpu_usage, 0.0);
    assert_eq!(st.memory_available, None);
    assert!(st.temperatures.is_empty());
    fs::remove_dir_all(&root).unwrap();
}
