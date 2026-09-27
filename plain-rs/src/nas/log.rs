//! File + stderr logger behind the standard `log` facade.
//!
//! Why not `tracing`?
//! ------------------
//! The `tracing` ecosystem is the de-facto Rust standard, but the call
//! sites in this codebase only ever use the lowest-level surface:
//! `info!` / `debug!` / `warn!` / `error!` / `trace!` through the
//! `log` crate macros.
//!
//! `tracing` + `tracing-subscriber` together pull in:
//!   * `tracing-core`, `tracing-log`, `tracing-attributes`
//!   * `tracing-subscriber` -> `matchers`, `regex-automata`, `regex-syntax`,
//!     `aho-corasick`, `nu-ansi-term`, `sharded-slab`, `thread_local`,
//!     `lazy_static`
//!
//! That's ~13 crates and ~5 seconds of compile time for what is
//! effectively `eprintln!("[LEVEL] {message}")` plus a level filter.
//!
//! This module:
//!   1. Exposes `init(level)` — installs a [`log::Log`] implementation
//!      (stderr mirror + optional on-disk sink) via `log::set_boxed_logger`
//!      and sets both the facade's max level and the internal filter, so
//!      disabled-level `log::` call sites skip formatting entirely.
//!   2. Keeps the on-disk log sink (`<data_dir>/logs/latest.log`) behind
//!      the developer UI (`appLogPath` / `appLogs` / `clearAppLogs`).
//!
//! What we deliberately don't support:
//!   * spans, `event!`, `#[instrument]` (no call-sites use them).
//!   * structured JSON output (we never needed it).
//!   * per-target filters (one global level is enough).

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Log level. Ordered: Trace < Debug < Info < Warn < Error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
    fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "trace" => Level::Trace,
            "debug" => Level::Debug,
            "info" => Level::Info,
            "warn" | "warning" => Level::Warn,
            "error" => Level::Error,
            _ => Level::Info,
        }
    }
    fn to_filter(self) -> log::LevelFilter {
        match self {
            Level::Trace => log::LevelFilter::Trace,
            Level::Debug => log::LevelFilter::Debug,
            Level::Info => log::LevelFilter::Info,
            Level::Warn => log::LevelFilter::Warn,
            Level::Error => log::LevelFilter::Error,
        }
    }
}

static LEVEL: AtomicUsize = AtomicUsize::new(2 /* Level::Info */);

/// Initialise the logger and the global level from a string. `RUST_LOG`
/// overrides the explicit `level` argument. Installing the logger a
/// second time (tests) keeps the first installation.
pub fn init(level: &str) {
    let level = match std::env::var("RUST_LOG") {
        Ok(env) => Level::parse(&env),
        Err(_) => Level::parse(level),
    };
    let _ = log::set_logger(&NAS_LOGGER);
    log::set_max_level(level.to_filter());
    LEVEL.store(level as usize, Ordering::Relaxed);
}

#[inline]
pub fn enabled(level: Level) -> bool {
    // `LEVEL` is the minimum severity to show: anything at or above it
    // (Info ≤ Warn ≤ Error) is emitted, anything more verbose is not.
    // The old `<=` comparison was inverted — it showed trace/debug while
    // silently swallowing warn/error at the default `info` level.
    level as usize >= LEVEL.load(Ordering::Relaxed)
}

/// Current level. Tests save/restore it around level-sensitive assertions.
#[cfg_attr(not(test), allow(dead_code))]
pub fn level() -> Level {
    match LEVEL.load(Ordering::Relaxed) {
        0 => Level::Trace,
        1 => Level::Debug,
        2 => Level::Info,
        3 => Level::Warn,
        _ => Level::Error,
    }
}

/// Set the level at runtime (config reload, tests). Mirrors it into the
/// facade's max level so `log::` call sites below the threshold skip
/// formatting their arguments entirely.
#[cfg_attr(not(test), allow(dead_code))]
pub fn set_level(level: Level) {
    log::set_max_level(level.to_filter());
    LEVEL.store(level as usize, Ordering::Relaxed);
}

fn write_log(level: Level, body: &str) {
    if !enabled(level) {
        return;
    }
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{lvl:>5} {body}", lvl = level.as_str());
    append_to_file(level, body);
}

// ---------------------------------------------------------------------------
// On-disk log sink (`<data_dir>/logs/latest.log`), the surface behind the
// developer UI (`appLogPath` / `appLogs` / `clearAppLogs`), mirroring
// plain-app's DiskLog: a single `latest.log` file, one line per record,
// newest lines at the end.
//
// File line format: `YYYY-MM-DD HH:MM:SS.mmm LEVEL body` — same shape as
// plain-app's `DiskLogFormatStrategy`, but timestamps are UTC (project
// rule for anything persisted).
// ---------------------------------------------------------------------------

/// File name of the on-disk log, relative to `<data_dir>/logs/`.
const LOG_FILE_NAME: &str = "latest.log";

/// Default on-disk log path for a data dir. Single source of truth for
/// the startup sink, the `appLogPath` resolver and tests.
pub fn default_log_file(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join(LOG_FILE_NAME)
}

struct Sink {
    path: PathBuf,
    file: std::fs::File,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Enable the on-disk sink. Called once at startup once the data dir is
/// known; logging before that (or in subcommands that never call this)
/// still goes to stderr only.
pub fn set_file(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::File::options()
        .append(true)
        .create(true)
        .open(path)
    {
        Ok(file) => {
            *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(Sink {
                path: path.to_path_buf(),
                file,
            });
        }
        Err(e) => write_log(Level::Warn, &format!("log file sink open failed: {e}")),
    }
}

/// Append one rendered record to the on-disk sink. No-op when the sink
/// is disabled. A write error drops the cached handle and reopens once
/// (recovers from the file being rotated/deleted underneath us).
fn append_to_file(level: Level, body: &str) {
    let mut guard = SINK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(sink) = guard.as_mut() else { return };
    let ts = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("{ts} {lvl} {body}\n", lvl = level.as_str());
    if let Err(e) = sink
        .file
        .write_all(line.as_bytes())
        .and_then(|_| sink.file.flush())
    {
        let path = sink.path.clone();
        *guard = None;
        drop(guard);
        write_log(
            Level::Warn,
            &format!("log file write failed ({e}); reopening {}", path.display()),
        );
        set_file(&path);
    }
}

/// Truncate the on-disk log. When `path` is the active sink file the
/// cached handle is rewound too; otherwise the file is truncated
/// standalone. No-op when the file does not exist (plain-app parity).
pub fn clear_file(path: &Path) {
    let mut guard = SINK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(sink) = guard.as_mut()
        && sink.path == path
        && sink.file.set_len(0).is_ok()
    {
        let _ = sink.file.seek(SeekFrom::Start(0));
        return;
    }
    // No sink, a foreign path, or a stale handle: truncate standalone.
    if let Ok(f) = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
    {
        let _ = f.set_len(0);
    }
}

/// Read log lines from `path` in **newest-first** order without loading
/// the whole file: reads backwards in 64 KB blocks so only
/// `offset + limit` lines are ever in memory (plain-app `AppLogHelper`
/// semantics, byte-for-byte behavior parity: empty lines are skipped,
/// trailing `\r` trimmed, a file without a trailing newline still yields
/// its last line).
/// Newest-first log lines, keeping only lines that contain `needle`
/// (case-insensitive) when one is given; the backward block scan keeps
/// going until enough matching lines are collected or the file start is
/// reached, so filtering precedes offset/limit (plain-app
/// `appLogs(filter: TextFilter)`).
pub fn read_lines_newest_first(
    path: &Path,
    needle: Option<&str>,
    offset: usize,
    limit: usize,
) -> Vec<String> {
    let needle = needle.map(str::to_lowercase);
    let matches = |line: &str| match &needle {
        None => true,
        Some(n) => line.to_lowercase().contains(n.as_str()),
    };
    if limit == 0 {
        return Vec::new();
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return Vec::new();
    };
    let len = meta.len() as usize;
    if len == 0 {
        return Vec::new();
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };

    const BLOCK: usize = 64 * 1024;
    let needed = offset.saturating_add(limit);
    let mut collected: Vec<String> = Vec::with_capacity(needed);
    // Bytes of the not-yet-terminated earliest line carried over from the
    // previous (higher-offset) block.
    let mut tail: Vec<u8> = Vec::new();
    let mut read_pos = len;

    while read_pos > 0 && collected.len() < needed {
        let block_len = BLOCK.min(read_pos);
        read_pos -= block_len;
        let mut combined = vec![0u8; block_len + tail.len()];
        if file.seek(SeekFrom::Start(read_pos as u64)).is_err() {
            return collected;
        }
        if file.read_exact(&mut combined[..block_len]).is_err() {
            return collected;
        }
        combined[block_len..].copy_from_slice(&tail);

        let mut end = combined.len();
        for i in (0..combined.len()).rev() {
            if combined[i] == b'\n' {
                if i + 1 < end {
                    let line = String::from_utf8_lossy(&combined[i + 1..end])
                        .trim_end_matches('\r')
                        .to_string();
                    if !line.is_empty() && matches(&line) {
                        collected.push(line);
                        if collected.len() >= needed {
                            return collected.into_iter().skip(offset).take(limit).collect();
                        }
                    }
                }
                end = i;
            }
        }
        tail = combined[..end].to_vec();
    }

    if collected.len() < needed && !tail.is_empty() {
        let line = String::from_utf8_lossy(&tail)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        if !line.is_empty() && matches(&line) {
            collected.push(line);
        }
    }
    collected.into_iter().skip(offset).take(limit).collect()
}

// ---------------------------------------------------------------------------
// The `log` facade bridge — every call site (NAS modules and plain-rs
// media code alike) logs through the standard `log::` macros and lands
// here: level-filtered, mirrored to stderr, appended to the on-disk sink
// when enabled.
// ---------------------------------------------------------------------------

struct NasLogger;

static NAS_LOGGER: NasLogger = NasLogger;

impl log::Log for NasLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        enabled(from_facade_level(metadata.level()))
    }

    fn log(&self, record: &log::Record) {
        let level = from_facade_level(record.level());
        if !enabled(level) {
            return;
        }
        write_log(level, &format!("{}", record.args()));
    }

    fn flush(&self) {}
}

fn from_facade_level(level: log::Level) -> Level {
    match level {
        log::Level::Trace => Level::Trace,
        log::Level::Debug => Level::Debug,
        log::Level::Info => Level::Info,
        log::Level::Warn => Level::Warn,
        log::Level::Error => Level::Error,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/nas/log.rs"]
mod tests;
