//! Tiny logging shim — replaces `tracing` + `tracing-subscriber`.
//!
//! Why not `tracing`?
//! ------------------
//! The `tracing` ecosystem is the de-facto Rust standard, but the
//! 35+ call sites in this codebase only ever use the lowest-level
//! surface: `info!` / `debug!` / `warn!` / `error!` / `trace!` with
//! an optional `key = %expr, key = ?expr,` field prefix. No spans, no
//! `#[instrument]`, no `event!`, no `tracing-subscriber` registries.
//!
//! `tracing` + `tracing-subscriber` together pull in:
//!   * `tracing-core`, `tracing-log`, `tracing-attributes`
//!   * `tracing-subscriber` -> `matchers`, `regex-automata`, `regex-syntax`,
//!     `aho-corasick`, `nu-ansi-term`, `sharded-slab`, `thread_local`,
//!     `lazy_static`
//!   * the `log` crate (transitive, just for re-exports)
//!
//! That's ~13 crates and ~5 seconds of compile time for what is
//! effectively `eprintln!("[LEVEL] {message}")` plus a level filter.
//!
//! This module:
//!   1. Exposes `init()` and `panic_err()` with the same signatures the
//!      old `tracing-subscriber`-based shim had.
//!   2. Re-exports `info!`, `debug!`, `warn!`, `error!`, `trace!` as
//!      local `macro_rules!` macros so call-sites that used
//!      `tracing::info!` can be rewritten to `log::info!` with no
//!      behaviour change.
//!
//! Supported call shapes (the actual call-sites use only these):
//!   * `info!("msg {var}")`                   — Display, like `format!`
//!   * `info!("static msg")`                  — static string
//!   * `info!(key = %expr, ..., "msg {var}")` — fields then format string
//!   * `info!(key = ?expr, ..., "msg {var}")` — fields then format string
//!   * `info!(bare_field, ..., "msg")`        — bare fields (Display)
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
}

static LEVEL: AtomicUsize = AtomicUsize::new(2 /* Level::Info */);

/// Initialise the global level from a string. `RUST_LOG` overrides the
/// explicit `level` argument.
pub fn init(level: &str) {
    install_facade();
    if let Ok(env) = std::env::var("RUST_LOG") {
        LEVEL.store(Level::parse(&env) as usize, Ordering::Relaxed);
    } else {
        LEVEL.store(Level::parse(level) as usize, Ordering::Relaxed);
    }
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

/// Set the level at runtime (config reload, tests).
#[cfg_attr(not(test), allow(dead_code))]
pub fn set_level(level: Level) {
    LEVEL.store(level as usize, Ordering::Relaxed);
}

#[doc(hidden)]
pub fn write_log(level: Level, body: &str) {
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

#[doc(hidden)]
pub fn emit(level: Level, body: String) {
    write_log(level, &body);
}

// ---------------------------------------------------------------------------
// The five user-facing macros. They accept the `tracing`-style call
// shapes used in this codebase and forward to `log::emit` with a fully
// rendered `String`.
//
// Field rendering is inlined into the call site at compile time: each
// `key = %expr` becomes `format!("key={} ", expr)`, each `bare_field`
// becomes `format!("bare_field={} ", bare_field)`, etc. We then call
// `format!` on the trailing message.
// ---------------------------------------------------------------------------

#[macro_export]
macro_rules! __log_trace {
    (target: $target:expr, $($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Trace) {{
            let mut body = format!("[target={}] ", $target);
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Trace, body);
        }}
    }};
    ($($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Trace) {{
            let mut body = String::new();
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Trace, body);
        }}
    }};
}

#[macro_export]
macro_rules! __log_debug {
    (target: $target:expr, $($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Debug) {{
            let mut body = format!("[target={}] ", $target);
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Debug, body);
        }}
    }};
    ($($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Debug) {{
            let mut body = String::new();
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Debug, body);
        }}
    }};
}

#[macro_export]
macro_rules! __log_info {
    (target: $target:expr, $($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Info) {{
            let mut body = format!("[target={}] ", $target);
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Info, body);
        }}
    }};
    ($($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Info) {{
            let mut body = String::new();
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Info, body);
        }}
    }};
}

#[macro_export]
macro_rules! __log_warn {
    (target: $target:expr, $($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Warn) {{
            let mut body = format!("[target={}] ", $target);
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Warn, body);
        }}
    }};
    ($($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Warn) {{
            let mut body = String::new();
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Warn, body);
        }}
    }};
}

#[macro_export]
macro_rules! __log_error {
    (target: $target:expr, $($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Error) {{
            let mut body = format!("[target={}] ", $target);
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Error, body);
        }}
    }};
    ($($rest:tt)+) => {{
        if $crate::log::enabled($crate::log::Level::Error) {{
            let mut body = String::new();
            $crate::__log_render!(@acc body $($rest)+);
            $crate::log::emit($crate::log::Level::Error, body);
        }}
    }};
}

/// Internal helper used by the level macros above. Walks the
/// `tracing`-style argument list and renders fields + message into
/// `$acc` (a `String` binding the caller provides).
#[macro_export]
#[doc(hidden)]
macro_rules! __log_render {
    // bare field + comma + more
    (@acc $acc:ident $name:ident, $($rest:tt)+) => {{
        $acc.push_str(&format!("{key}={val} ", key = stringify!($name), val = $name));
        $crate::__log_render!(@acc $acc $($rest)+);
    }};
    // bare field with `?` prefix (Debug) + comma + more
    (@acc $acc:ident ?$name:ident, $($rest:tt)+) => {{
        $acc.push_str(&format!("{key}={val:?} ", key = stringify!($name), val = $name));
        $crate::__log_render!(@acc $acc $($rest)+);
    }};
    // key = %expr + comma + more
    (@acc $acc:ident $key:ident = %$val:expr, $($rest:tt)+) => {{
        $acc.push_str(&format!("{key}={v} ", key = stringify!($key), v = $val));
        $crate::__log_render!(@acc $acc $($rest)+);
    }};
    // key = ?expr + comma + more
    (@acc $acc:ident $key:ident = ?$val:expr, $($rest:tt)+) => {{
        $acc.push_str(&format!("{key}={v:?} ", key = stringify!($key), v = $val));
        $crate::__log_render!(@acc $acc $($rest)+);
    }};
    // key = literal + comma + more
    (@acc $acc:ident $key:ident = $val:expr, $($rest:tt)+) => {{
        $acc.push_str(&format!("{key}={v} ", key = stringify!($key), v = $val));
        $crate::__log_render!(@acc $acc $($rest)+);
    }};
    // terminator: format-string + args
    (@acc $acc:ident $fmt:literal $(, $arg:expr)* $(,)?) => {{
        $acc.push_str(&format!($fmt $(, $arg)*));
    }};
}

// Re-export the macros under aliases so they appear under
// `crate::log::info!` etc. (and to dodge a name collision with
// the transitive `log` crate, which also exports `info!`/`warn!`/etc.
// at crate root). Call-sites that want our shim should reference it as
// `crate::log::info!(...)`.
//
// We intentionally do NOT publish the macro under the unprefixed
// `info` / `warn` / etc. name in this module because the `log` crate
// already exports them at crate root and that confuses resolution.
// Re-export the macros under their original names so call-sites can
// use `crate::log::info!` / `crate::log::warn!` / etc. The transitive
// `log` crate also exports `info!`/`warn!`/etc. at crate root, which
// causes name resolution ambiguity inside this `pub use` expression;
// we resolve it by using a module-qualified path.
pub use crate::__log_debug as debug;
pub use crate::__log_error as error;
pub use crate::__log_info as info;
pub use crate::__log_warn as warn;

#[cfg(test)]
#[path = "../tests/unit/log.rs"]
mod tests;

// ---------------------------------------------------------------------------
// `log` crate facade bridge — plain-rs media code logs through the
// standard `log::` macros; route them into this file logger so NAS logs
// stay in one place.
// ---------------------------------------------------------------------------

struct FacadeLogger;

impl log::Log for FacadeLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let level = match metadata.level() {
            log::Level::Trace => Level::Trace,
            log::Level::Debug => Level::Debug,
            log::Level::Info => Level::Info,
            log::Level::Warn => Level::Warn,
            log::Level::Error => Level::Error,
        };
        enabled(level)
    }

    fn log(&self, record: &log::Record) {
        let level = match record.level() {
            log::Level::Trace => Level::Trace,
            log::Level::Debug => Level::Debug,
            log::Level::Info => Level::Info,
            log::Level::Warn => Level::Warn,
            log::Level::Error => Level::Error,
        };
        if !enabled(level) {
            return;
        }
        let target = record.target();
        let body = if target.is_empty() {
            format!("{}", record.args())
        } else {
            format!("[{target}] {}", record.args())
        };
        write_log(level, &body);
    }

    fn flush(&self) {}
}

static FACADE: FacadeLogger = FacadeLogger;

/// Install the `log` crate facade → file logger bridge. Idempotent —
/// a second call (tests) is a no-op.
fn install_facade() {
    let _ = log::set_logger(&FACADE);
    log::set_max_level(log::LevelFilter::Trace);
}
